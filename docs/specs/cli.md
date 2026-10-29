# CLI surface

Status: Draft

One binary, `gdt`, with subcommands (ADR 0003). Global flags: `--json` (machine-readable output where the command prints data), `--quiet`, `--verbose`, `--color <auto|always|never>`, `--godot <version>` (override the resolved version for this command).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic failure |
| 2 | Usage error |
| 3 | Network or download failure |
| 4 | Version not found or not installed |
| 5 | Checksum verification failed |
| 10 | Tests failed (`gdt test`) |
| other | `gdt run` and the shim forward Godot's own exit code unchanged |

## Versions

See [version-management.md](version-management.md).

| Command | Purpose |
|---|---|
| `gdt install [<version>] [--mono]` | Download and install a build. With no version, installs what the project resolves to |
| `gdt uninstall <version>` | Remove an installed build |
| `gdt list [--remote] [--all]` | Installed builds; `--remote` queries GitHub; `--all` includes pre-releases |
| `gdt use <version>` | Set the global default |
| `gdt pin <version>` | Write the version into the project's `gdt.toml` |
| `gdt which [<project>]` | Print the path of the executable that would run |
| `gdt run [-- <godot args>]` | Run the resolved Godot with the given arguments, from the current folder |
| `gdt open [<path>]` | Open the editor on a `project.godot` (or its folder) with the version it asks for |
| `gdt shim install` / `gdt shim uninstall` | Manage the `godot` shim on `PATH` |
| `gdt upgrade [<version>]` | Move a project to another engine version (milestone M6, to be researched) |

## Addons

See [addons.md](addons.md).

| Command | Purpose |
|---|---|
| `gdt add <source>` | Add an addon to `gdt.toml`, install it, update the lockfile |
| `gdt remove <name>` | Remove from the manifest, the lockfile and `addons/` |
| `gdt install` | Install exactly what the lockfile says (when addons are declared) |
| `gdt update [<name>]` | Re-resolve to newer versions and rewrite the lockfile |
| `gdt search <text>` | Search the Asset Library sources |
| `gdt addons` | List installed addons and their sources |

`gdt install` with no arguments does both jobs in order: ensure the project's Godot build is installed, then sync addons.

## Tests

See [testing.md](testing.md).

| Command | Purpose |
|---|---|
| `gdt test [<paths or filters>]` | Run the project's test suites headless |

## Utilities (later)

`gdt doctor` (check the environment), `gdt self-update`, `gdt completions <shell>`, `gdt config get|set`.
