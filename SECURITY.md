# Security policy

`gdt` downloads and runs executables and unpacks archives, so security reports matter.

Please report vulnerabilities privately through GitHub's **Security → Report a vulnerability** form on this repository rather than in a public issue. Expect an acknowledgement within a few days.

In scope: checksum or signature verification bypasses, archive extraction path traversal, addon names or folders from `gdt.toml`, `gdt.lock` or the command line that reach outside `addons/`, secrets (such as `GITHUB_TOKEN`) appearing in output or sent to a host other than `api.github.com`, unsafe handling of downloaded content, and shim behavior that could run an unintended executable.

There is no published release yet; once there is one, only the latest release is supported.
