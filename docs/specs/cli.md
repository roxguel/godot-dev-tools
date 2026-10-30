# CLI surface

Status: Draft. The command structure is decided in [ADR 0007](../adr/0007-command-structure.md); `list`, `list --remote`, `inspect` and `paths` exist today.

One binary, `gdt` (ADR 0003). Commands are grouped by area, like `dotnet tool install` or `gh auth login`:

- `engine` for Godot builds, with **shortcuts** at the top level for the commands people use all the time.
- `addon` for addons. Always spelled out, never a shortcut.
- Project actions at the top level (`run`, `open`, `test`, `sync`, `inspect`, `init`).
- `auth`, `shim`, `config`, `cache` and a few utilities.

Running `gdt` with no arguments prints the help and exits with 0. It never launches anything.

A top-level word that belongs to another group fails with a hint: `gdt add 4.0` answers "there is no `add`; did you mean `gdt addon add` or `gdt install`?".

## Global flags

`--progress <auto|rich|plain|none>`, `--quiet`, `--color <auto|always|never>`, `--verbose`, `--json` (commands that print data), `--godot <version>` (override the resolved engine for this command).

## Output and progress

Data goes to **stdout**; progress and status messages go to **stderr**, so pipes and scripts see clean data. `--json` output never contains progress.

Progress comes in three styles, chosen with `--progress <auto|rich|plain|none>` (default `auto`):

| Style | What it looks like | Chosen by `auto` when |
|---|---|---|
| `rich` | Spinners and progress bars with speed and ETA, redrawn in place | stderr is a terminal, `CI` is not set and `TERM` is not `dumb` |
| `plain` | One line per step (`Fetching Godot releases...`, `done`), no redrawing | anything else (pipes, logs, CI) |
| `none` | Nothing | never; used by `--quiet` |

`--quiet` is the same as `--progress none` and also hides informational messages (errors are always shown). `NO_COLOR` and `--color` are honored. The library only reports progress through a small trait; drawing it is the CLI's job, so the library and the shim never print (ADR 0001, ADR 0006).

## Network

- GitHub API requests send `User-Agent: gdt/<version>`.
- An optional `GITHUB_TOKEN` (or `GH_TOKEN`) raises the API rate limit. It is only ever sent to `api.github.com`, never to other hosts, and never printed.
- Release lists are cached under the cache directory for 10 minutes; `--refresh` skips the cache.
- When the rate limit is hit, `gdt` says so, when it resets, and suggests setting a token; exit code 3.
- Credentials for other services are described in [authentication.md](authentication.md).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic failure |
| 2 | Usage error |
| 3 | Network or download failure |
| 4 | Version not found or not installed |
| 5 | Checksum verification failed |
| 6 | Authentication required or rejected |
| 10 | Tests failed (`gdt test`) |
| other | `gdt run` and the shim forward Godot's own exit code unchanged |

## Engine

See [version-management.md](version-management.md). Each command below works as `gdt engine <command>`; the ones marked **shortcut** also work as `gdt <command>`.

A *version spec* is `latest`, `beta`, `rc`, `dev`, `pre`, `4.7`, `4.7.2`, `4.8-rc1`, each with an optional `-mono` suffix (`gdt install 4.7.2-mono`). `--mono` is accepted as the same thing.

| Command | Example | What it does |
|---|---|---|
| `install [spec]` (**shortcut**) | `gdt install 4.7.2`, `gdt install latest-mono`, `gdt install beta`, `gdt install` | Download, verify and install a build. With no spec, installs what the project asks for. The first build installed becomes the global default if there is none |
| `uninstall <spec>` (**shortcut**) | `gdt uninstall 4.6.3` | Remove an installed build |
| `list` / `ls` (**shortcut**) | `gdt ls`, `gdt ls --remote --all --json` | Installed builds; `--remote` lists what GitHub offers; `--all` adds pre-releases; `--refresh` skips the cache |
| `use <spec>` (**shortcut**) | `gdt use 4.7.2` | Set the global default. Stores the concrete version, never an alias |
| `pin <spec>` (**shortcut**) | `gdt pin latest` | Write the build this project uses into `gdt.toml`. Stores the concrete version |
| `which` | `gdt which` | Print the path of the executable that would run here (for scripts) |
| `info [spec]` | `gdt engine info` | Executable, version, flavor, the editor settings folder, the project data folder and whether the build is self-contained |
| `upgrade [spec]` | `gdt upgrade 4.8` | Move a project to another engine version using Godot's own tools (to be researched, milestone M6) |

Channel aliases resolve to the newest published build of that channel: `latest` is the newest stable, `beta`, `rc` and `dev` the newest of each pre-release channel, `pre` the newest pre-release of any channel.

## Project

| Command | Example | What it does |
|---|---|---|
| `run [script.gd] [-- args]` | `gdt run tools/x.gd`, `gdt run -- --path . -s tools/x.gd` | Run Godot with the resolved version from any folder. With a `.gd` file it is the same as `godot --headless -s <file>` (like `node index.js`); after `--`, arguments go to Godot untouched. Output and exit code are forwarded |
| `open [path]` | `gdt open`, `gdt open C:\games\cozy`, `gdt open --manager` | Open the editor on a project. Outside a project, or with `--manager`, opens Godot's project manager with the default build |
| `inspect [path]` | `gdt inspect` | Show which engine a project asks for (exists) |
| `init` | `gdt init` | Create a `gdt.toml` from the engine the project already uses |
| `sync` | `gdt sync` | Make a fresh clone ready: install the engine and the addons from the project files (like `npm ci`) |
| `test [filter]` | `gdt test`, `gdt test tests/test_x.gd` | Run the project's test suites headless. See [testing.md](testing.md) |

## Addons

See [addons.md](addons.md). Always under `gdt addon`.

| Command | Example | What it does |
|---|---|---|
| `add <source>` | `gdt addon add publisher/asset`, `gdt addon add github:bitwes/Gut@v9.4.0`, `gdt addon add library:1234` | Add an addon to `gdt.toml`, install it and update the lockfile |
| `install [source]` | `gdt addon install`, `gdt addon install --locked` | With no source, install exactly what the lockfile says (`--locked` fails instead of changing the lockfile). With a source, the same as `add` |
| `remove` / `rm` `<name>` | `gdt addon rm gut` | Remove from the manifest, the lockfile and `addons/` |
| `update [name]` | `gdt addon update` | Look for newer versions and rewrite the lockfile |
| `list` / `ls` | `gdt addon ls` | Installed addons and where they came from |
| `search <text>` | `gdt addon search dialogue` | Search the Asset Store and the Asset Library |

## System

| Command | Example | What it does |
|---|---|---|
| `auth login store` | `gdt auth login store`, `gdt auth login store --with-token < token.txt` | Save an Asset Store API token. See [authentication.md](authentication.md) |
| `auth status` / `auth logout store` | `gdt auth status` | Show which services are signed in; forget a token (and revoke it when asked) |
| `shim install` / `shim uninstall` | `gdt shim install --modify-path` | Install or remove the `godot` shim; `--modify-path` also puts it on `PATH` |
| `config get\|set\|list` | `gdt config set auto_install true` | Read or change user settings |
| `cache clean` | `gdt cache clean` | Delete downloaded archives and cached release lists |
| `paths` | `gdt paths` | Where `gdt` keeps its files (exists) |
| `doctor` | `gdt doctor` | Check the environment: `PATH`, shim, .NET, permissions |
| `self-update` | `gdt self-update` | Update `gdt` |
| `completions <shell>` | `gdt completions powershell` | Generate shell completions |
