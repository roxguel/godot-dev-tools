# Documentation site

The site at <https://gdt.roxguel.dev>, built with [Astro Starlight](https://starlight.astro.build). It lives in the same repository as the tool so that a change to the behavior and to its documentation go in one pull request.

## Where the content comes from

| What | Source | How |
|---|---|---|
| Specs, ADRs, vision, roadmap, open questions, the command reference | `../docs/` | `scripts/sync-docs.mjs` |
| Changelog, security policy, contributing | `../CHANGELOG.md`, `../SECURITY.md`, `../CONTRIBUTING.md` | `scripts/sync-docs.mjs` |
| Home, installation, quick start, guides | `src/content/docs/` | written by hand |
| `/install` (the installers, readable in the browser) | `public/install.sh`, `public/install.ps1` | read at build time by `src/pages/install.astro` |

`sync-docs.mjs` runs before every `dev` and `build`; after the build, `scripts/check-links.mjs` fails on any internal link that points nowhere. It writes Starlight pages under `src/content/docs/` (git-ignored; see `.gitignore`), taking the title from the first `# ` line, pointing "Edit this page" at the real file in `docs/`, and rewriting relative links (to another synced document: a site route; to anything else in the repository: a GitHub URL). It fails on a link to a file that does not exist. **Edit the source files, never the generated pages.**

## The installers are a public contract

`public/install.sh` and `public/install.ps1` are served at `https://gdt.roxguel.dev/install.sh` and `/install.ps1`, and people paste those URLs into scripts and CI. Do not move or rename them. There is one copy of each, here. The `Installers` job in `.github/workflows/ci.yml` checks them with ShellCheck and a PowerShell parse.

## Working on it

Node is pinned in `.nvmrc`, pnpm in `package.json` (`packageManager`).

```bash
pnpm install --frozen-lockfile
pnpm dev                    # live reload (search needs a build)
pnpm build && pnpm preview  # the real thing, with search
```

## Supply chain

- `pnpm-lock.yaml` is committed and CI installs with `--frozen-lockfile`.
- `pnpm-workspace.yaml` sets `minimumReleaseAge` (3 days) and blocks dependency install scripts; `allowBuilds` lists the only ones that may run (none today).
- Dependabot opens the updates (npm ecosystem, `/site`); CI runs `pnpm audit`.
- No analytics and no third-party scripts: search is Pagefind, built into Starlight.

## Deploy

`.github/workflows/site.yml` builds on every pull request that touches `site/`, `docs/` or the root documents the site reuses (check only, no deploy), and deploys to GitHub Pages only on a push to `main`. The custom domain (`gdt.roxguel.dev`) is set in the repository's Settings → Pages, with the source set to GitHub Actions; GitHub ignores a `CNAME` file for deploys made by Actions, so `public/CNAME` only documents the domain.
