---
title: Tests in CI with GitHub Actions
description: Run a Godot project's GUT, gdUnit4 or script tests headless on GitHub Actions with gdt.
sidebar:
  order: 4
---

`gdt test` runs the project's existing test framework headless with the project's Godot version. It imports a fresh
clone first, judges the result from the JUnit report and not from the exit code alone, and exits with **10** when tests
fail (0 when they pass), so a CI step fails the way you expect.

## Locally

```sh
gdt test                              # the runner found from addons/, or [test] in gdt.toml
gdt test test/unit                    # only some folders or files
gdt test --report junit.xml           # also copy the JUnit file
```

GUT has been run for real with `gdt`. gdUnit4 support follows its documentation and has not been run against a real
project yet; plain script suites work (`[test] runner = "script"`).

## On GitHub Actions

```yaml title=".github/workflows/tests.yml"
name: Tests

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  test:
    runs-on: ubuntu-latest
    env:
      GITHUB_TOKEN: ${{ github.token }}   # raises the GitHub API rate limit for downloads
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1

      - name: Install gdt
        run: |
          curl -fsSL https://gdt.roxguel.dev/install.sh | sh -s -- --version 0.1.0
          echo "$HOME/.local/bin" >> "$GITHUB_PATH"

      - name: Install Godot and the addons
        run: gdt sync

      - name: Run the tests
        run: gdt test --report junit.xml

      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2
        if: always()
        with:
          name: junit
          path: junit.xml
```

Notes:

- Pin the `gdt` version (`--version`) so a new release never changes your CI by surprise, and pin third-party actions to
  a full commit SHA, as above.
- `gdt sync` reads the Godot version and the addons from `gdt.toml` and `gdt.lock`. The project needs no other CI
  configuration.
- On a Windows runner use `irm https://gdt.roxguel.dev/install.ps1 | iex` in a `pwsh` step and add
  `$env:LOCALAPPDATA\Programs\gdt` to `$env:GITHUB_PATH`.
- Do not give a workflow that runs pull requests from forks any secret.

Details: [testing spec](/specs/testing/), [exit codes](/reference/cli/#exit-codes).
