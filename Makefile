# Makefile provides GNU Make aliases for the repository task entrypoints.
# Makefile 用于为仓库任务入口提供 GNU Make 别名。

.PHONY: build release run deps host lua

# build forwards to the shell-native task entrypoint for a debug build.
# build 用于转发到 shell 原生任务入口，执行 debug 构建。
build:
	@bash ./make.sh build

# release forwards to the shell-native task entrypoint for a release build.
# release 用于转发到 shell 原生任务入口，执行 release 构建。
release:
	@bash ./make.sh release

# run forwards to the shell-native task entrypoint for debug execution.
# run 用于转发到 shell 原生任务入口，执行 debug 运行。
run:
	@bash ./make.sh run

# deps installs all dependencies unless a specific dependency target is also requested.
# deps 用于安装全部依赖；如果同时指定了具体依赖目标，则保持分组兼容语义。
deps:
ifeq ($(filter host lua,$(MAKECMDGOALS)),)
	@bash ./make.sh deps
else
	@:
endif

# host maps to the host-level native dependency bootstrap flow.
# host 用于映射宿主级原生依赖初始化流程。
host:
	@bash ./make.sh deps host

# lua maps to the Lua runtime dependency bootstrap flow only.
# lua 用于映射仅包含 Lua runtime 的依赖初始化流程。
lua:
	@bash ./make.sh deps lua
