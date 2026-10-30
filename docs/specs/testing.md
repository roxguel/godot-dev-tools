# Testing

Status: Implemented for GUT and for a test script; gdUnit4 is implemented from its own documentation and source but has **not been run** (see below).

`gdt test` does not define a test framework. It runs the project's existing one, headless, with the project's Godot version, and normalizes the result.

## Configuration

```toml
[test]
runner = "gut"            # "gut" | "gdunit4" | "script"; found from the installed addons if omitted
paths = ["test"]          # folders or test files, relative to the project (default: "test")
script = "tools/tests/run_suites.gd"   # only for runner = "script"
```

## Detection

1. `[test] runner`, or `--runner` on the command line.
2. `addons/gut/plugin.cfg` present: GUT. `addons/gdUnit4/plugin.cfg` present: gdUnit4.
3. If both are present, `[test] runner` is required and `gdt test` says so.
4. `[test] script` and no addon: a script run with `--headless -s`, which must exit non-zero on failure.

A project with its own harness, such as Project Cozy's `tools/tests/run_suites.gd`, uses `runner = "script"`. Suites that need the editor open (for example the MCP-based `McpTestSuite`) are out of scope.

## Behavior

1. Find the project (from the current folder upward) and its Godot build as for `gdt run`: the project's `gdt.toml`, `project.godot` or the default. The console executable is used so the output reaches the terminal.
2. **Import first when needed.** If `.godot/global_script_class_cache.cfg` does not exist (a fresh clone, a CI runner), run `godot --headless --path <project> --import`. Without it GUT cannot load its own classes; and GUT still exits with 0 in that case.
3. Run the runner with the command line below and let its output through.
4. Read the JUnit report, because the exit code alone is not trustworthy.
5. Print a one-line summary (`Tests: 3, passed: 2, failed: 1, skipped: 0`) and return the exit code.

`gdt test [paths...]`: paths typed on the command line are relative to where `gdt` runs (a folder or a `.gd` file); the ones in `gdt.toml` are relative to the project. `--report FILE` copies the JUnit report to `FILE` (its folder is created).

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Tests ran and all passed |
| 10 | At least one test failed or errored |
| 1 | No tests were run (also what a runner that ran nothing reports), or an error from `gdt` |
| 2 | The setup is wrong: no runner, both runners, an unknown runner, a path outside the project or one that does not exist, a runner that is not installed |
| 4 | The Godot build is not installed |
| other | The runner ended before reporting anything (a crash, a parse error in a test): Godot's exit code, with a message |

### How the result is judged

| Report | Exit code | Outcome |
|---|---|---|
| failures or errors > 0 | any | failed (10) |
| 0 tests | any | nothing ran (1) |
| tests > 0, no failures | non-zero | failed (10) |
| tests > 0, no failures | 0 | passed |
| none (GUT) | 0 | nothing ran (1): GUT prints "Nothing was run" and exits 0 |
| none (GUT) | non-zero | the runner failed |
| none (script) | 0 / non-zero | passed / failed (10) |

## Command lines

**GUT** (9.7.1, run for real with Godot 4.7.2):

```
godot --headless --path <project> -s addons/gut/gut_cmdln.gd \
      -gdir=res://test -ginclude_subdirs -gexit \
      -gjunit_xml_file=res://.godot/gdt-tests/junit.xml
```

Folders go to `-gdir` (comma separated) and test files to `-gtest`. Arguments such as `-gdir=res://test` must reach Godot as one word: in PowerShell they have to be quoted when typed by hand, because PowerShell splits a word that starts with `-` at the `:`. `gdt` passes them untouched.

**gdUnit4** (6.2.0; taken from the addon's own `runtest.sh` and its command-line parser, **not run** because executing the addon's script was declined during development):

```
godot --headless --path <project> -s -d --remote-debug tcp://127.0.0.1:0 \
      res://addons/gdUnit4/bin/GdUnitCmdTool.gd \
      --add res://test --ignoreHeadlessMode --continue -rd res://.godot/gdt-tests/gdunit4
```

`gdt` reads no report for gdUnit4 yet (it writes its JUnit file inside a numbered folder under `-rd`), so its result is the exit code of the runner alone. Reading its report is the next step once the command has been run for real.

**Script:** `godot --headless --path <project> -s res://<script>`.

## CI shape

```bash
gdt sync                 # the engine and the addons from the project files
gdt test --report junit.xml
```
