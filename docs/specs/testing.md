# Testing

Status: Draft. Runner command lines are assumptions to verify.

`gdt test` does not define a test framework. It runs the project's existing one, headless, with the project's Godot version, and normalizes the result.

## Configuration

```toml
[test]
runner = "gut"                  # "gut" | "gdunit4" | "script"; auto-detected if omitted
paths = ["tests"]
timeout = 600                   # seconds
report = "junit"                # writes target/gdt/test-report.xml
```

## Detection

1. `[test] runner` if set.
2. `addons/gdUnit4` present: gdUnit4.
3. `addons/gut` present: GUT.
4. `[test] script`: a user script run with `--headless --script`, which must exit non-zero on failure.

## Behavior

- Resolve the Godot version as usual and run it with `--headless --path <project>` and the runner's entry point.
- Import first when the project has never been imported (`--import`), so a clean CI checkout works.
- Pass filters through: `gdt test tests/test_missions.gd` runs only that suite.
- Forward the runner's output; add a one-line summary at the end.
- Exit code 0 when all pass, 10 when any test fails, 4 when the engine is missing, and Godot's code for crashes.
- Write a JUnit XML report when asked (`--report junit`), converting the runner's native report if needed.

## CI shape

```bash
gdt install        # engine and addons from the project files
gdt test --report junit
```

## Custom runners

Projects with their own harness (for example an editor-plugin based one) can use the `script` runner. If their suites need the editor to be open, they are out of scope for `gdt test`.
