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

`--progress <auto|rich|plain|none>`, `-q`/`--quiet`, `-v`/`--verbose`, `--color <auto|always|never>`, `-g`/`--godot <version>` (use this installed build for this command, whatever the project or the default say; it must be installed), and `-j`/`--json` on commands that print data.

### Short options

A command-line option gets a one-letter form when it is used often and harmless: `-a` `--all`, `-r` `--remote`, `-c` `--channel`, `-m` `--mono`, `-j` `--json`, `-e` `--editor`, `-o` `--open`, `-g` `--godot`, `-q` `--quiet`, `-v` `--verbose`. The same letter always means the same thing, and the global letters (`q`, `v`, `g`, plus `-h` and `-V`) are never reused. Options that are rare, that change or delete things, or that are easy to type by accident stay long-only (`--refresh`, `--manager`, `--locked`, `--no-dev`, `--revoke`, `--with-token`, `--modify-path`). `-f` is kept for a future `--force`, and `-D` for `--dev`.

`-v` prints diagnostics on stderr, prefixed `[debug]`: what is being resolved, URLs requested and their HTTP status, cache hits, the asset chosen, checksums, folders used. `-vv` adds what the underlying libraries log (HTTP and TLS). Progress switches to plain lines while `-v` is on, so the diagnostics stay readable. Tokens and other secrets are never logged. The library reports diagnostics through the `log` crate and the CLI decides whether to show them.

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

A *version spec* is `latest`, `beta`, `rc`, `dev`, `pre`, `4`, `4.7`, `4.7.2`, `4.8-rc1`, each with an optional `-mono` suffix (`gdt install 4.7.2-mono`). `--mono` is accepted as the same thing.

| Command | Example | What it does |
|---|---|---|
| `install [spec]` (**shortcut**) | `gdt install 4.7.2`, `gdt install latest-mono`, `gdt install beta`, `gdt install` | Download, verify and install a build. With no spec, installs what the project asks for. The first build installed becomes the global default if there is none |
| `uninstall <spec>` (**shortcut**) | `gdt uninstall 4.6.3` | Remove an installed build. The spec must name it exactly (or a `MAJOR.MINOR` line with only one build installed); aliases such as `latest` are refused so nothing is deleted by a moving name. If it was the default, the default is cleared |
| `list` / `ls` (**shortcut**) | `gdt ls`, `gdt ls -r`, `gdt ls -ra`, `gdt ls -r -c rc,beta`, `gdt ls -m`, `gdt ls -q` | Builds as lines of `<gutter> <version> <tags>`. `*` in the gutter means installed (the rest are available upstream). Tags: `in use here` (the build that applies in this folder, green and bold), `default` (the global default), `latest`, `rc`, `beta`, `dev` (the newest release of that channel, the aliases `install` accepts; remote only), and for a remote version the flavors installed (`standard (default), mono`). A line on stderr says where the build in use came from, and a project that asks for a build that is not installed gets a note. `-r`/`--remote` lists what GitHub offers (stable only); `-a`/`--all` adds rc, beta and dev; `-c`/`--channel` picks channels (`stable`, `rc`, `beta`, `dev`, `pre` = the three pre-release ones; comma separated; wins over `--all`) and only asks GitHub for the repositories it needs; installed builds are all shown unless `--channel` is given; `-m`/`--mono` keeps only the .NET builds (and, with `--remote`, marks only the .NET ones as installed); `-q` prints bare ids, one per line, for scripts; `-j`/`--json` adds `default`, `in_use`, `in_use_from` and, remotely, `aliases`; `--refresh` skips the cache. Colors follow `--color` and `NO_COLOR` |
| `current` (**shortcut**) | `gdt current`, `gdt current -q`, `gdt current -j` | Print the build in use here (for example `4.7.2-mono`), the one `run` and `open` would use. Unlike `gdt use` with no spec, which shows the global default |
| `use [spec]` (**shortcut**) | `gdt use 4.7.2`, `gdt use` | Choose the default among the **installed** builds (`gdt use latest` is the newest installed). Stores the concrete version, never an alias. Without a spec, shows the current default. It changes what `gdt run` and `gdt open` use outside a project; to make a plain `godot` command follow it, install the shim (`gdt shim install`) |
| `pin <spec>` (**shortcut**) | `gdt pin latest` | Write the build this project uses into `gdt.toml` (`[godot] version`). Looks the spec up on GitHub when it is not exact, and stores the concrete version. Says if that build is not installed yet |
| `which [path]` (**shortcut**) | `gdt which`, `gdt which --editor` | Print the executable that would run here, for scripts. On Windows it is the console one unless `--editor` is given |
| `info [spec]` | `gdt engine info` | Executable, version, flavor, the editor settings folder, the project data folder and whether the build is self-contained |
| `upgrade [spec]` | `gdt upgrade 4.8` | Move a project to another engine version using Godot's own tools (to be researched, milestone M6) |

Channel aliases resolve to the newest published build of that channel: `latest` is the newest stable, `beta`, `rc` and `dev` the newest of each pre-release channel, `pre` the newest pre-release of any channel.

## Project

| Command | Example | What it does |
|---|---|---|
| `run [script.gd] [args]` | `gdt run tools/x.gd one two`, `gdt run -- --path . -s tools/x.gd` | Run Godot with the resolved version from any folder, with its output and exit code passed through (like `node index.js`). A first argument ending in `.gd` is a script: it runs as `godot --headless -s <file> -- <args>`, and the script reads its arguments with `OS.get_cmdline_user_args()`. Anything else is given to Godot untouched, so `gdt run -- --path <dir> ...` works from any folder: `--path` also decides which project's version is used. On Windows it runs the `_console.exe` build, the only one that prints to a terminal. Checked with 4.7.2: output, script arguments and exit code 3 from `quit(3)` |
| `open [path]` | `gdt open`, `gdt open C:\games\cozy`, `gdt open --manager` | Open the editor on a project. Outside a project, or with `--manager`, opens Godot's project manager with the default build. It returns at once and the editor keeps running when the terminal closes (detached on Windows; elsewhere it follows the terminal's own rules). A path that is not inside a project is an error |
| `inspect [path]` | `gdt inspect` | Show which engine a project asks for (exists) |
| `init` | `gdt init` | Create a `gdt.toml` from the engine the project already uses |
| `sync [--no-dev]` | `gdt sync`, `gdt sync --no-dev` | Make a fresh clone ready: install the engine and the addons from the project files (like `npm ci`). With `--no-dev`, skip the development addons |
| `test [paths]` | `gdt test`, `gdt test test/unit`, `gdt test test/test_x.gd --report junit.xml` | Run the project's tests headless with the right Godot: GUT, gdUnit4 or a script of your own, found from the installed addons or `[test]` in `gdt.toml`. Imports a fresh clone first, and judges the result from the JUnit report rather than the exit code alone. Exits 0 when all pass, 10 when any fails. See [testing.md](testing.md) |

## Addons

See [addons.md](addons.md). Always under `gdt addon`.

| Command | Example | What it does |
|---|---|---|
| `add <source>` | `gdt addon add publisher/asset`, `gdt addon add github:bitwes/Gut@v9.4.0 --dev`, `gdt addon add library:1234` | Add an addon to `gdt.toml`, install it and update the lockfile. `--dev` marks it as a development addon |
| `install [source]` | `gdt addon install`, `gdt addon install --locked --no-dev` | With no source, install exactly what the lockfile says (`--locked` fails instead of changing the lockfile; `--no-dev` skips development addons). With a source, the same as `add` |
| `remove` / `rm` `<name>` | `gdt addon rm gut` | Remove from the manifest, the lockfile and `addons/` |
| `update [name]` | `gdt addon update` | Look for newer versions and rewrite the lockfile |
| `list` / `ls` | `gdt addon ls` | Installed addons and where they came from |
| `search <text>` | `gdt addon search dialogue`, `gdt addon search dialogue --library` | Search the Asset Store (or, with `--library`, the legacy Asset Library). Works anywhere; inside a project it prefers what works with its Godot. Prints a table with a header (`ASSET`, `NAME`, `LICENSE` and `PRICE`, or `VERSION` for the Library): `publisher/asset` (or `library:id`), name, license, and free or paid (or the version and its Godot). `-q` prints only the ids, `-j` JSON |

## System

| Command | Example | What it does |
|---|---|---|
| `auth login store` | `gdt auth login store`, `gdt auth login store --with-token < token.txt` | Save an Asset Store API token. See [authentication.md](authentication.md) |
| `auth status` / `auth logout store` | `gdt auth status` | Show which services are signed in; forget a token (and revoke it when asked) |
| `shim install` / `uninstall` / `status` | `gdt shim install`, `gdt shim install --modify-path`, `gdt shim status` | Put the `godot` shim in gdt's bin folder (idempotent; refreshes an old copy), remove it, or show where it stands: installed and up to date, on `PATH` or not, and which `godot` would run. `gdt` changes `PATH` only with `--modify-path` (on `install`, and undone with `uninstall --modify-path`); see [version-management.md](version-management.md) and ADR 0009 |
| `config get\|set\|list` | `gdt config set auto_install true` | Read or change user settings |
| `cache clean` | `gdt cache clean` | Delete downloaded archives and cached release lists |
| `paths [--open <what>]` | `gdt paths`, `gdt paths --open data`, `gdt paths --open config-file` | Where `gdt` keeps its files; with `--open`, show one in the file manager. `<what>` is `data`, `config`, `config-file` (selected in its folder), `versions`, `cache` or `bin`. A file that does not exist yet opens its folder; a missing folder is an error (exists) |
| `doctor` | `gdt doctor` | Check the environment: `PATH`, shim, .NET, permissions |
| `self-update` | `gdt self-update` | Update `gdt` |
| `completions <shell>` | `gdt completions powershell` | Generate shell completions |
