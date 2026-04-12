# vulcan-mcp

MCP (Model Context Protocol) server for the Vulcan ecosystem. Supports STDIO and HTTP transports, with gRPC integration for LanceDb (vector search), SQLite (database), and VMM (VulcanMemoryMesh).

## Features

- **MCP Protocol** — Supports versions 2025-11-25 (primary), 2025-03-26, and 2024-11-05
- **Transport modes** — STDIO (default) and HTTP Streamable + Legacy SSE
- **LanceDb gRPC** — Vector table creation, upsert, search, delete, and drop
- **SQLite gRPC** — SQL execution (single/batch/stream) and JSON queries
- **VMM gRPC** — 23 RPC methods for memory management, profile, and scratchpad operations (connected but not exposed as MCP tools; reserved for separate integration)
- **VMCP Scratchpad** — DWM working memory via SQLite with `vmcp_` prefixed tables, supporting plan locks, key-value upsert/delete/get, and atomic transactions

## Architecture

```
src/
├── main.rs           # Entry point: config loading, server builder
├── config.rs         # YAML config loader (-config flag, fallback to config.yaml)
├── server.rs         # MCP server core: tool/resource/prompt registration, message handling
├── grpc_client.rs    # gRPC clients: LanceDb, SQLite, VMM + ScratchpadStore
├── http_server.rs    # HTTP transport (Streamable + Legacy SSE) and STDIO runner
├── protocol.rs       # MCP JSON-RPC types, version negotiation, feature flags
└── session.rs        # HTTP session management
proto/
└── v1/
    ├── lancedb.proto # LanceDb service definition
    ├── sqlite.proto  # SQLite service definition
    └── vmm.proto     # VMM service definition (VulcanMemoryMesh)
```

## Quick Start

### Build

```bash
cargo build --release
```

### Configuration

Create a `config.yaml` file (placed next to the executable, or specified via `-config` flag):

```yaml
# HTTP transport (optional — if absent, falls back to STDIO)
http: "0.0.0.0:3000"

# LanceDb gRPC endpoint (optional)
lancedb: "http://localhost:50051"

# SQLite gRPC endpoint (optional)
# Also enables vmcp_scratchpad working memory store
sqlite: "http://localhost:50052"

# VMM gRPC endpoint (optional)
# Connected but not exposed as MCP tools
vmm: "http://localhost:50053"
```

### Run with STDIO transport (default)

```bash
./vulcan-mcp
# or with explicit config
./vulcan-mcp -config /path/to/config.yaml
```

### Run with HTTP transport

Set `http` in `config.yaml` and start. The server exposes:

| Method | Path | Description |
|--------|------|-------------|
| POST   | `/mcp` | HTTP Streamable: send/receive JSON-RPC |
| GET    | `/sse` | Legacy SSE: establish SSE stream |
| POST   | `/message` | Legacy SSE: send messages to the server |

## Available MCP Tools

### Built-in
| Tool | Description |
|------|-------------|
| `add` | Add two numbers |
| `greet` | Greet someone by name |
| `current_time` | Get current UTC time |

### LanceDb (vector)
| Tool | Description |
|------|-------------|
| `lancedb_create_table` | Create a LanceDb table with columns |
| `lancedb_upsert` | Upsert data (JSON rows or Arrow IPC) |
| `lancedb_search` | Vector similarity search |
| `lancedb_delete` | Delete rows by condition |
| `lancedb_drop_table` | Drop a table |

### SQLite (database)
| Tool | Description |
|------|-------------|
| `sqlite_execute` | Execute a single SQL statement |
| `sqlite_execute_batch` | Batch execute with parameter arrays |
| `sqlite_query` | Query with JSON or Arrow IPC output |

### VMCP Scratchpad (working memory)
| Tool | Description |
|------|-------------|
| `vmcp_scratchpad_upsert` | Write key/value anchors (single or batch) |
| `vmcp_scratchpad_delete` | Delete keys from scratchpad |
| `vmcp_scratchpad_get` | Read scratchpad items |
| `vmcp_scratchpad_list_keys` | List all keys in current plan |
| `vmcp_scratchpad_clean` | Clear entire scratchpad scope |

## Testing

### STDIO mode

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}' | ./vulcan-mcp
```

### HTTP mode

```bash
# Initialize
curl -X POST http://localhost:3000/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{}}}'

# List tools
curl -X POST http://localhost:3000/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'

# SSE stream (legacy)
curl -N http://localhost:3000/sse
```

## License

MIT
