-- ========================================
-- 中文：这是 Lua 行备注，用于验证双横线注释摘要、装饰线过滤以及中英文压缩能力。
-- English: This Lua line comment validates double-dash summary extraction and compaction.
-- @param value 这个标签不应进入最终摘要 / This tag must not appear in the final summary.
local function build_skill_name(value)
    return (value or ""):gsub("^%s+", ""):gsub("%s+$", "")
end

--[[
----------------------------------------
Español: Este bloque de comentario de Lua valida el filtrado de separadores y el resumen multilingüe.
English: This Lua block comment validates multilingual summarization and separator filtering.
@returns Esta etiqueta no debe aparecer en el resumen final / This tag must not appear in the final summary.
]]
local function normalize_skill_name(value)
    return (value or ""):lower()
end
