use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::lua_skill::SkillMeta;
use crate::protocol::{Tool, ToolAnnotations};

// ============================================================
// Loaded skill (compiled Lua function + metadata)
// ============================================================

struct LoadedSkill {
    meta: SkillMeta,
}

// ============================================================
// LuaEngine — LuaJIT VM wrapper
// ============================================================

pub struct LuaEngine {
    lua: Arc<Mutex<Lua>>,
    skills: HashMap<String, LoadedSkill>,
}

impl LuaEngine {
    /// Create a new LuaEngine with LuaJIT VM and registered globals.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let lua = Lua::new();

        // Configure package paths to include project-local luarocks tree.
        Self::setup_package_paths(&lua)?;

        // Register the `vulcan` module (placeholder — will be populated
        // with cross-skill call and filesystem helpers after skills load).
        Self::register_vulcan_module(&lua)?;

        // cjson and lfs are loaded via package.cpath from third_party/lua_packages
        // (installed by scripts/install_lua_deps.ps1). No simulated fallback needed.

        Ok(Self {
            lua: Arc::new(Mutex::new(lua)),
            skills: HashMap::new(),
        })
    }

    /// Load skills from directories. `base_dir` is the system skill directory,
    /// `override_dir` is the user override directory (if any).
    pub fn load_from_dirs(
        &mut self,
        base_dir: &Path,
        override_dir: Option<&Path>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !base_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(base_dir)? {
            let entry = entry?;
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }

            let skill_name = entry.file_name().to_string_lossy().to_string();

            // Check override directory
            let actual_dir = if let Some(od) = override_dir {
                let override_skill_dir = od.join(&skill_name);
                if override_skill_dir.exists() {
                    // Empty directory = disable this skill
                    if override_skill_dir.read_dir()?.next().is_none() {
                        eprintln!("[LuaSkill] Disabled by empty override: {}", skill_name);
                        continue;
                    }
                    eprintln!("[LuaSkill] Override loaded: {}", skill_name);
                    override_skill_dir
                } else {
                    eprintln!("[LuaSkill] System loaded: {}", skill_name);
                    skill_dir
                }
            } else {
                eprintln!("[LuaSkill] System loaded: {}", skill_name);
                skill_dir
            };

            if let Err(e) = self.load_single_skill(&actual_dir) {
                eprintln!("[LuaSkill] Failed to load {}: {}", skill_name, e);
            }
        }

        // After loading all skills, populate the vulcan.call function with
        // references to loaded skill functions.
        self.populate_vulcan_call()?;

        eprintln!("[LuaSkill] {} skills loaded", self.skills.len());
        Ok(())
    }

    /// Load a single skill from its directory.
    fn load_single_skill(&mut self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let skill_json = dir.join("skill.json");
        if !skill_json.exists() {
            return Err(format!("skill.json not found in {}", dir.display()).into());
        }

        let json_str = std::fs::read_to_string(&skill_json)?;
        let meta: SkillMeta = serde_json::from_str(&json_str)?;

        let lua_path = dir.join(&meta.lua_entry);
        if !lua_path.exists() {
            return Err(format!("Lua entry {} not found in {}", meta.lua_entry, dir.display()).into());
        }

        let lua_source = std::fs::read_to_string(&lua_path)?;

        // Compile and register the Lua function.
        // main.lua typically does `return function(args) ... end`, so the chunk
        // is an outer wrapper. We call it once to get the inner handler function,
        // then store that handler for skill invocations.
        let lua = self.lua.lock().unwrap();
        let chunk = lua.load(&lua_source).set_name(&meta.lua_module);
        let outer: Function = chunk.into_function()?;

        // Call the outer chunk once to retrieve the inner skill handler.
        let handler: Function = outer.call(())
            .map_err(|e| format!("Failed to initialize skill '{}': {}", meta.lua_module, e))?;

        // Store the inner handler in Lua globals for later invocation.
        let globals = lua.globals();
        globals.set(format!("__skill_{}", meta.lua_module), handler)?;

        self.skills.insert(meta.tool_name.clone(), LoadedSkill { meta });

        Ok(())
    }

    /// Return MCP Tool definitions for all loaded skills.
    pub fn list_skills(&self) -> Vec<Tool> {
        self.skills.values().map(|s| {
            let mut desc = s.meta.description.clone();
            if !s.meta.prompt.is_empty() {
                desc.push_str("\n\n");
                desc.push_str(&s.meta.prompt);
            }

            let mut props = serde_json::Map::new();
            let mut required = Vec::new();
            for p in &s.meta.parameters {
                let mut prop = serde_json::Map::new();
                prop.insert("type".to_string(), Value::String(p.param_type.clone()));
                prop.insert("description".to_string(), Value::String(p.description.clone()));
                props.insert(p.name.clone(), Value::Object(prop));
                if p.required {
                    required.push(p.name.clone());
                }
            }

            Tool::with_annotations(
                &s.meta.tool_name,
                &desc,
                Value::Object(props),
                required,
                ToolAnnotations {
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    user_confirmation_required: Some(false),
                    idempotent_hint: Some(true),
                },
            )
        }).collect()
    }

    /// Check if a tool_name is a Lua skill.
    pub fn is_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    /// Call a loaded Lua skill with the given JSON arguments.
    /// This is synchronous — wrap in spawn_blocking for async contexts.
    pub fn call_skill(&self, tool_name: &str, args: &Value) -> Result<Value, String> {
        let module_name = self.skills.get(tool_name)
            .ok_or_else(|| format!("Lua skill '{}' not found", tool_name))?
            .meta.lua_module.clone();

        let lua = self.lua.lock().unwrap();
        let func_name = format!("__skill_{}", module_name);
        let func: Function = lua.globals().get(func_name.as_str())
            .map_err(|e| format!("Skill function '{}' not found: {}", module_name, e))?;

        // Convert JSON args to Lua table
        let args_table = json_to_lua_table(&lua, args)?;

        // Call the function
        let result: LuaValue = func.call(args_table)
            .map_err(|e| format!("Lua skill '{}' error: {}", module_name, e))?;

        // Convert result back to JSON
        lua_value_to_json(&result)
    }

    /// Execute arbitrary Lua code and return the result.
    pub fn run_lua(&self, code: &str, args: &Value) -> Result<Value, String> {
        let lua = self.lua.lock().unwrap();

        // Build a wrapper that passes args as a local variable
        let args_table = json_to_lua_table(&lua, args)?;
        lua.globals().set("__runlua_args", args_table)
            .map_err(|e| format!("Failed to set args: {}", e))?;

        let wrapper = format!("return (function()\n  local args = __runlua_args\n  {}\nend)()", code);

        let result = lua.load(&wrapper).eval::<LuaValue>()
            .map_err(|e| format!("Lua execution error: {}", e))?;

        lua_value_to_json(&result)
    }

    /// Populate the vulcan.call function to dispatch to loaded skills.
    fn populate_vulcan_call(&self) -> Result<(), String> {
        let lua = self.lua.lock().unwrap();
        let vulcan: Table = lua.globals().get("vulcan")
            .map_err(|e| format!("vulcan module not found: {}", e))?;

        // Create the call dispatcher
        let skills: Vec<(String, String)> = self.skills.iter()
            .map(|(name, s)| (name.clone(), s.meta.lua_module.clone()))
            .collect();

        let skill_names: Vec<String> = skills.iter().map(|(n, _)| n.clone()).collect();
        let module_names: Vec<String> = skills.iter().map(|(_, m)| m.clone()).collect();

        let dispatcher = lua.create_function(move |lua, (name, args): (String, Table)| {
            // Find the module function
            let idx = skill_names.iter().position(|n| n == &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Skill '{}' not found", name)))?;
            let module = &module_names[idx];
            let func_name = format!("__skill_{}", module);
            let func: Function = lua.globals().get(func_name.as_str())
                .map_err(|_| mlua::Error::runtime(format!("Skill function '{}' not found", module)))?;
            func.call::<LuaValue>(args)
        }).map_err(|e| format!("Failed to create vulcan.call dispatcher: {}", e))?;

        vulcan.set("call", dispatcher)
            .map_err(|e| format!("Failed to set vulcan.call: {}", e))?;

        Ok(())
    }

    /// Configure package.path and package.cpath to include project-local luarocks tree.
    /// This allows `require("cjson")` etc. to load C modules installed via luarocks.
    fn setup_package_paths(lua: &Lua) -> Result<(), Box<dyn std::error::Error>> {
        // Find the lua_packages directory relative to the executable's parent directory.
        let exe_path = std::env::current_exe().ok();
        if let Some(exe_path) = exe_path {
            if let Some(exe_dir) = exe_path.parent() {
                let parent = exe_dir.parent().unwrap_or(exe_dir);
                let lua_packages = parent.join("lua_packages");

                if lua_packages.exists() {
                    // Build package.cpath entries for C modules (.dll on Windows)
                    #[cfg(windows)]
                    let cpath_pattern = format!(
                        "{}\\share\\lua\\5.1\\?.dll;{}\\share\\lua\\5.1\\?\\init.dll;{}\\lib\\lua\\5.1\\?.dll;{}\\lib\\lua\\5.1\\loadall.dll;{}\\?\\?.dll;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Build package.path entries for Lua modules
                    #[cfg(windows)]
                    let path_pattern = format!(
                        "{}\\share\\lua\\5.1\\?.lua;{}\\share\\lua\\5.1\\?\\init.lua;{}\\?.lua;",
                        lua_packages.display(),
                        lua_packages.display(),
                        lua_packages.display()
                    );

                    // Prepend to existing paths
                    let package: Table = lua.globals().get("package")?;
                    let old_cpath: mlua::String = package.get("cpath")?;
                    let new_cpath = format!("{}{}", cpath_pattern, old_cpath.to_str()?.to_string());
                    package.set("cpath", lua.create_string(&new_cpath)?)?;

                    let old_path: mlua::String = package.get("path")?;
                    let new_path = format!("{}{}", path_pattern, old_path.to_str()?.to_string());
                    package.set("path", lua.create_string(&new_path)?)?;

                    eprintln!("[LuaEngine] package.cpath prepended: {}", lua_packages.display());
                }
            }
        }
        Ok(())
    }

    /// Register the `vulcan` module in the Lua VM.
    fn register_vulcan_module(lua: &Lua) -> Result<(), Box<dyn std::error::Error>> {
        let vulcan = lua.create_table()?;

        // vulcan.log(level, message)
        let log_fn = lua.create_function(|_, (level, msg): (String, String)| {
            eprintln!("[LuaSkill:{}] {}", level, msg);
            Ok(())
        })?;
        vulcan.set("log", log_fn)?;

        // vulcan.fs_list(dir) -> array of filenames
        let fs_list_fn = lua.create_function(|_, dir: String| {
            let entries: Vec<String> = std::fs::read_dir(&dir)
                .map_err(|e| mlua::Error::runtime(format!("fs_list: {}", e)))?
                .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
                .collect();
            Ok(entries)
        })?;
        vulcan.set("fs_list", fs_list_fn)?;

        // vulcan.fs_read(path) -> string
        let fs_read_fn = lua.create_function(|_, path: String| {
            std::fs::read_to_string(&path)
                .map_err(|e| mlua::Error::runtime(format!("fs_read: {}", e)))
        })?;
        vulcan.set("fs_read", fs_read_fn)?;

        // vulcan.fs_write(path, content)
        let fs_write_fn = lua.create_function(|_, (path, content): (String, String)| {
            std::fs::write(&path, content)
                .map_err(|e| mlua::Error::runtime(format!("fs_write: {}", e)))
        })?;
        vulcan.set("fs_write", fs_write_fn)?;

        // vulcan.fs_exists(path) -> bool
        let fs_exists_fn = lua.create_function(|_, path: String| {
            Ok(std::path::Path::new(&path).exists())
        })?;
        vulcan.set("fs_exists", fs_exists_fn)?;

        // vulcan.fs_is_dir(path) -> bool
        let fs_is_dir_fn = lua.create_function(|_, path: String| {
            Ok(std::path::Path::new(&path).is_dir())
        })?;
        vulcan.set("fs_is_dir", fs_is_dir_fn)?;

        // vulcan.path_join(...strings) -> string
        let path_join_fn = lua.create_function(|lua, parts: MultiValue| {
            let mut parts_vec = Vec::new();
            for val in parts.into_iter() {
                if let LuaValue::String(s) = val {
                    if let Ok(borrowed) = s.to_str() {
                        parts_vec.push(borrowed.to_string());
                    }
                }
            }
            if parts_vec.is_empty() {
                return lua.create_string("");
            }
            #[cfg(windows)]
            let result = parts_vec.join("\\");
            #[cfg(not(windows))]
            let result = parts_vec.join("/");
            lua.create_string(&result)
        })?;
        vulcan.set("path_join", path_join_fn)?;

        // vulcan.json_encode(table) -> string
        let json_encode_fn = lua.create_function(|lua, val: LuaValue| {
            match lua_value_to_json(&val) {
                Ok(json) => lua.create_string(serde_json::to_string(&json).unwrap_or_default()),
                Err(e) => Err(mlua::Error::runtime(format!("json_encode: {}", e))),
            }
        })?;
        vulcan.set("json_encode", json_encode_fn)?;

        // vulcan.json_decode(string) -> table
        let json_decode_fn = lua.create_function(|lua, s: String| {
            match serde_json::from_str::<Value>(&s) {
                Ok(json) => json_to_lua_table_inner(lua, &json),
                Err(e) => Err(mlua::Error::runtime(format!("json_decode: {}", e))),
            }
        })?;
        vulcan.set("json_decode", json_decode_fn)?;

        // Placeholder for call (populated after skills load)
        let call_stub = lua.create_function(|_, _: (String, Table)| {
            Err::<(), _>(mlua::Error::runtime("vulcan.call not initialized"))
        })?;
        vulcan.set("call", call_stub)?;

        lua.globals().set("vulcan", vulcan)?;

        Ok(())
    }
}

// ============================================================
// JSON ↔ Lua Value conversion
// ============================================================

fn json_to_lua_table(lua: &Lua, json: &Value) -> Result<Table, String> {
    json_to_lua_table_inner(lua, json).map_err(|e| e.to_string())
}

fn json_to_lua_table_inner(lua: &Lua, json: &Value) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    if let Value::Object(obj) = json {
        for (k, v) in obj {
            table.set(k.as_str(), json_value_to_lua(lua, v)?)?;
        }
    } else if let Value::Array(arr) = json {
        for (i, v) in arr.iter().enumerate() {
            table.set(i + 1, json_value_to_lua(lua, v)?)?;
        }
    }
    Ok(table)
}

fn json_value_to_lua(lua: &Lua, json: &Value) -> mlua::Result<LuaValue> {
    match json {
        Value::Null => Ok(LuaValue::Nil),
        Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else {
                Ok(LuaValue::Number(n.as_f64().unwrap_or(0.0)))
            }
        }
        Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        Value::Array(_) | Value::Object(_) => {
            Ok(LuaValue::Table(json_to_lua_table_inner(lua, json)?))
        }
    }
}

fn lua_value_to_json(val: &LuaValue) -> Result<Value, String> {
    match val {
        LuaValue::Nil => Ok(Value::Null),
        LuaValue::Boolean(b) => Ok(Value::Bool(*b)),
        LuaValue::Integer(i) => Ok(Value::Number((*i).into())),
        LuaValue::Number(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                Ok(Value::Number(n))
            } else {
                Ok(Value::Null)
            }
        }
        LuaValue::String(s) => {
            Ok(Value::String(s.to_str().map(|b| b.to_string()).unwrap_or_default()))
        }
        LuaValue::Table(t) => {
            // Try as array first (sequential integer keys starting at 1)
            if let Ok(arr) = lua_table_to_array(t) {
                return Ok(Value::Array(arr));
            }
            // Fall back to object
            lua_table_to_object(t)
        }
        LuaValue::Function(_) => Err("Cannot convert Lua function to JSON".to_string()),
        LuaValue::Thread(_) => Err("Cannot convert Lua thread to JSON".to_string()),
        LuaValue::UserData(_) => Err("Cannot convert Lua userdata to JSON".to_string()),
        LuaValue::LightUserData(_) => Err("Cannot convert light userdata to JSON".to_string()),
        _ => Err("Unknown Lua value type".to_string()),
    }
}

fn lua_table_to_array(t: &Table) -> Result<Vec<Value>, String> {
    let len = t.raw_len();
    if len == 0 {
        // Could be empty object or empty array, default to array
        return Ok(Vec::new());
    }
    let mut arr = Vec::with_capacity(len);
    for i in 1..=len {
        let val: LuaValue = t.get(i).map_err(|e| format!("Array index {}: {}", i, e))?;
        arr.push(lua_value_to_json(&val)?);
    }
    Ok(arr)
}

fn lua_table_to_object(t: &Table) -> Result<Value, String> {
    let mut obj = serde_json::Map::new();
    for pair in t.pairs::<String, LuaValue>() {
        let (k, v) = pair.map_err(|e| format!("Table key: {}", e))?;
        obj.insert(k, lua_value_to_json(&v)?);
    }
    Ok(Value::Object(obj))
}
