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

# deps is a grouping alias so commands like `make deps host` remain valid.
# deps 用作分组别名，使 `make deps host` 这类命令保持可用。
deps:
	@:

# host maps to the host-level native dependency bootstrap flow.
# host 用于映射宿主级原生依赖初始化流程。
host:
	@bash ./make.sh deps host

# lua maps to the Lua dependency bootstrap flow, which also coordinates host deps.
# lua 用于映射 Lua 依赖初始化流程，并会协调宿主依赖初始化。
lua:
	@bash ./make.sh deps lua
