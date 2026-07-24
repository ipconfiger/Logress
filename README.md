# graftail

> Real-time Loki log tailing via Grafana Data Source Proxy — like `tail -f` for your cloud logs.

`graftail` connects to Loki through Grafana's proxy API, streams logs over WebSocket, and displays them in a readable, colorized format. No direct access to Loki or Elasticsearch needed — all authentication goes through Grafana.

## Install

### npm (recommended)

```bash
npm install -g graftail
```

The installer detects your platform and downloads the correct binary automatically.

### Binary download

Download the latest binary from [GitHub Releases](https://github.com/ipconfiger/Logress/releases) and place it in your `$PATH`.

### Build from source

```bash
git clone https://github.com/ipconfiger/Logress.git
cd Logress
cargo build --release
cp target/release/graftail /usr/local/bin/
```

## Configure

`graftail` reads config from environment variables, a `.env` file, or a config file. Pick one:

### Option 1: `.env` file (simplest)

```bash
# .env in your working directory, or ~/.config/graftail/.env
GRAFTAIL_URL=https://grafana.example.com
GRAFTAIL_DATASOURCE_UID=your-loki-uid
GRAFTAIL_USER=your-username
GRAFTAIL_PASSWORD=your-password
```

Then just run:

```bash
graftail -q prod-api --insecure
```

### Option 2: Environment variables

```bash
export GRAFTAIL_URL=https://grafana.example.com
export GRAFTAIL_DATASOURCE_UID=your-loki-uid
export GRAFTAIL_USER=your-username
export GRAFTAIL_PASSWORD=your-password
```

### Option 3: Config file

```toml
# ~/.config/graftail/config.toml
[graftail]
grafana_url = "https://grafana.example.com"
datasource_uid = "your-loki-uid"

[auth]
user = "your-username"
# password goes in env var or .env, not here
```

### Auth options

| Method | How |
|--------|-----|
| **Token** | `--token glsa_xxx` or `GRAFTAIL_TOKEN` env var |
| **Username + password** | `--user` / `--password` or env vars |
| **Interactive** | If only username is set, you'll be prompted for the password |

## Usage

### Discover available services

```bash
graftail list --insecure
```

### Tail a service in real time

```bash
# Short form — auto-expands to {service_name="prod-api"}
graftail -q prod-api --insecure

# Full LogQL
graftail -q '{service_name="prod-api"}' --insecure
graftail -q '{service_name="prod-api"} |= "error"' --insecure
```

### Show recent history then live tail

```bash
graftail -q prod-api --last 50 --insecure
```

### Filter by time

```bash
graftail -q prod-api --since 1h --insecure
```

### Common options

| Flag | Description |
|------|-------------|
| `-q` | Service name (short) or full LogQL query |
| `--last N` | Fetch N historical lines before tailing |
| `--since 1h` | Start tail from a relative time |
| `--output json` | Machine-readable JSON output |
| `--insecure` | Skip TLS verification (internal CAs) |
| `--utc` | Show timestamps in UTC |
| `-h` | Press `h` during tail to freeze/unfreeze output |

## License

MIT
