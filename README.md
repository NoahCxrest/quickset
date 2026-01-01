# quickset

a high-performance search database written in rust, optimized for extremely fast lookups across billions of rows.

because elasticsearch is bloated as shit and you just want to search some fucking data.

## features

- **hash index**: o(1) exact match lookups (~34ns per operation)
- **inverted index**: full-text search with tokenization
- **trie index**: prefix search capabilities  
- **sorted index**: range queries with binary search
- **bloom filter**: fast existence checks with configurable false positive rates
- **http api**: clickhouse-style rest interface
- **authentication**: username/password with role-based access control and configurable auth levels
- **logging**: configurable log levels (trace, debug, info, warn, error)
- **docker**: ready to deploy with docker and docker-compose

## performance

benchmarks on 1 million rows (on a machine that's not a literal potato):

| operation | time |
|-----------|------|
| exact search | ~20µs |
| range search | ~4µs |
| get by id | ~10ns |
| fulltext search | ~223µs |

your mileage may vary. if you're running this on a raspberry pi, god help you.

## installation

```bash
cargo build --release
```

## quick start

### run locally

```bash
cargo run --release
```

### run with docker

```bash
# build and run
docker build -t quickset .
docker run -p 8080:8080 quickset

# or use docker-compose
docker-compose up quickset

# with write-only auth (public reads, protected writes)
docker-compose up quickset-write-auth

# full paranoid mode (auth on everything)
docker-compose up quickset-full-auth
```

## configuration

all configuration is done via environment variables:

| variable | default | description |
|----------|---------|-------------|
| `QUICKSET_HOST` | `0.0.0.0` | bind address |
| `QUICKSET_PORT` | `8080` | port number |
| `QUICKSET_AUTH_LEVEL` | `none` | auth level (see below) |
| `QUICKSET_ADMIN_USER` | `admin` | admin username |
| `QUICKSET_ADMIN_PASS` | `admin` | admin password (change this you idiot) |
| `QUICKSET_LOG` | `info` | log level (trace/debug/info/warn/error/off) |
| `QUICKSET_MAX_CONN` | `1000` | max connections |

### sync configuration (clickhouse)

quickset can periodically sync data from clickhouse (or other sources in the future).

| variable | default | description |
|----------|---------|-------------|
| `QUICKSET_SYNC_ENABLED` | `false` | enable sync |
| `QUICKSET_SYNC_SOURCE` | `clickhouse` | source type |
| `QUICKSET_SYNC_HOST` | `localhost` | source host |
| `QUICKSET_SYNC_PORT` | `8123` | source port (http interface) |
| `QUICKSET_SYNC_USER` | `default` | source username |
| `QUICKSET_SYNC_PASSWORD` | | source password |
| `QUICKSET_SYNC_DATABASE` | `default` | source database |
| `QUICKSET_SYNC_INTERVAL` | `300` | sync interval in seconds (0 = manual only) |
| `QUICKSET_SYNC_TABLES` | | tables to sync (see format below) |

#### table format

tables are specified as comma-separated strings:

```
source_table:target_table:col1=type,col2=type
```

example:
```bash
QUICKSET_SYNC_TABLES="users:users:id=int,name=string,email=string,products:products:id=int,title=string,price=float"
```

types: `int`, `float`, `string`, `bytes`

### auth levels

you can configure how much of your api is locked down:

| level | reads | writes | health | use case |
|-------|-------|--------|--------|----------|
| `none` | open | open | open | local dev, yolo mode |
| `write` | open | auth | open | public reads, protected writes |
| `read` | auth | auth | open | all data ops need auth |
| `all` | auth | auth | auth | full lockdown, even health check |

backwards compatible with `QUICKSET_AUTH=true/false` (maps to `all`/`none`).

### example configurations

```bash
# no auth (living dangerously)
QUICKSET_AUTH_LEVEL=none cargo run --release

# protect your writes but let anyone search
QUICKSET_AUTH_LEVEL=write QUICKSET_ADMIN_PASS=secret cargo run --release

# lock everything down
QUICKSET_AUTH_LEVEL=all QUICKSET_ADMIN_PASS=supersecret cargo run --release
```

## authentication

when auth is required for an endpoint, use http basic auth.

### roles

| role | read | write | admin |
|------|------|-------|-------|
| `admin` | ✓ | ✓ | ✓ |
| `readwrite` | ✓ | ✓ | ✗ |
| `readonly` | ✓ | ✗ | ✗ |

### user management

```bash
# add user (admin only)
curl -u admin:admin -X POST http://localhost:8080/auth/user/add \
  -d '{"username":"bob","password":"secret","role":"readwrite"}'

# remove user (admin only)
curl -u admin:admin -X POST http://localhost:8080/auth/user/remove \
  -d '{"username":"bob"}'

# list users (admin only)
curl -u admin:admin http://localhost:8080/auth/users
```

## http api

### create table

```bash
curl -X POST http://localhost:8080/table/create \
  -H "Content-Type: application/json" \
  -d '{
    "name": "users",
    "columns": [
      {"name": "id", "type": "int"},
      {"name": "name", "type": "string"},
      {"name": "email", "type": "string"}
    ],
    "capacity": 1000000
  }'
```

### insert data

```bash
curl -X POST http://localhost:8080/insert \
  -H "Content-Type: application/json" \
  -d '{
    "table": "users",
    "rows": [
      [1, "alice", "alice@example.com"],
      [2, "bob", "bob@example.com"]
    ]
  }'
```

### exact search

```bash
curl -X POST http://localhost:8080/search \
  -d '{"table":"users","column":"name","type":"exact","value":"alice"}'
```

### prefix search

```bash
curl -X POST http://localhost:8080/search \
  -d '{"table":"users","column":"name","type":"prefix","prefix":"al"}'
```

### full-text search

```bash
curl -X POST http://localhost:8080/search \
  -d '{"table":"users","column":"name","type":"fulltext","query":"alice bob"}'
```

### range search

```bash
curl -X POST http://localhost:8080/search \
  -d '{"table":"users","column":"id","type":"range","min":1,"max":100}'
```

### get by ids

```bash
curl -X POST http://localhost:8080/get \
  -d '{"table":"users","ids":[1,2,3]}'
```

### update

```bash
curl -X POST http://localhost:8080/update \
  -d '{"table":"users","id":1,"values":[1,"alice updated","new@email.com"]}'
```

### delete

```bash
curl -X POST http://localhost:8080/delete \
  -d '{"table":"users","ids":[1,2]}'
```

### stats

```bash
curl http://localhost:8080/stats
```

### health check

```bash
curl http://localhost:8080/health
```

## sync api

if sync is configured, you can check status and trigger manual syncs.

### sync status

```bash
curl http://localhost:8080/sync/status
```

returns:
```json
{
  "success": true,
  "data": {
    "tables": [
      {
        "table": "users",
        "last_sync_ago_secs": 120,
        "last_row_count": 50000,
        "last_duration_ms": 234,
        "error": null,
        "syncing": false
      }
    ],
    "running": true,
    "total_syncs": 42
  }
}
```

### trigger sync (admin only)

```bash
# sync all tables
curl -u admin:admin -X POST http://localhost:8080/sync/trigger

# sync specific table
curl -u admin:admin -X POST http://localhost:8080/sync/trigger \
  -d '{"table":"users"}'
```

## running tests

```bash
# unit tests
cargo test

# integration tests  
cargo test --release

# stress tests (1m+ rows) - go get a coffee
cargo test --release million_row -- --ignored --nocapture
```

## running benchmarks

```bash
cargo bench
```

## architecture

```
src/
├── lib.rs          # module exports
├── main.rs         # http server entry
├── config.rs       # environment configuration (auth levels live here)
├── auth.rs         # authentication & authorization
├── log.rs          # logging system
├── storage.rs      # row storage (hashmap-based)
├── index.rs        # index implementations
│   ├── HashIndex       # o(1) exact match
│   ├── InvertedIndex   # full-text search
│   ├── TrieIndex       # prefix search
│   ├── SortedIndex     # range queries
│   └── BloomFilter     # existence checks
├── search.rs       # search engine coordination
├── table.rs        # table & database management
├── query.rs        # request/response types
└── http.rs         # http server & routing
```

## column types

- `int` / `integer` / `i64`
- `float` / `double` / `f64`
- `string` / `text` / `varchar`
- `bytes` / `blob` / `binary`

## search types

| type | use case | index used |
|------|----------|------------|
| `exact` | find exact matches | hashindex + bloomfilter |
| `prefix` | find strings starting with | trieindex |
| `fulltext` | search tokenized text | invertedindex |
| `range` | find values in range | sortedindex |
| `contains` | find substring | invertedindex (term) |

## why quickset?

- **minimal dependencies**: just serde. that's it. no tokio, no actix, no framework bullshit.
- **fast as fuck**: designed from the ground up for search performance
- **simple**: one binary, env vars for config, json api. done.
- **actually works**: unlike your last three side projects

## license

mit - do whatever you want, just don't blame me when it breaks.
