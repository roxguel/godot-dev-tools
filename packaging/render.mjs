// Writes the package manager manifests for one release of gdt.
//
//   node packaging/render.mjs <version> <SHA256SUMS> <out-dir> [release-date]
//
// <version> is the one in the tag without the "v" (0.1.0); <SHA256SUMS> is the file of
// that release; the manifests go to <out-dir>/winget, <out-dir>/scoop and
// <out-dir>/homebrew. The release workflow runs it and opens pull requests with the
// result in the tap and in the bucket (see packaging/README.md).

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [version, sumsFile, outDir, date = new Date().toISOString().slice(0, 10)] = process.argv.slice(2);
if (!version || !sumsFile || !outDir) {
  console.error("usage: node packaging/render.mjs <version> <SHA256SUMS> <out-dir> [release-date]");
  process.exit(2);
}
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`not a stable version: ${version}`);
  process.exit(2);
}

const REPO = "roxguel/godot-dev-tools";
const DESCRIPTION = "Godot Development Tools: version manager, addon manager and test runner for Godot.";
// Homebrew wants a description under 80 characters that does not start with the name.
const BREW_DESCRIPTION = "Version manager, addon manager and test runner for Godot";
const base = `https://github.com/${REPO}/releases/download/v${version}`;

// The archive of each target, with the SHA-256 the release published for it.
const archive = (target, ext) => `gdt-v${version}-${target}.${ext}`;
const sums = new Map();
for (const line of readFileSync(sumsFile, "utf8").split(/\r?\n/)) {
  const match = /^([0-9a-f]{64}) [ *](\S+)$/.exec(line);
  if (match) sums.set(match[2], match[1]);
}
const sha = (file) => {
  const hash = sums.get(file);
  if (!hash) {
    console.error(`${file} is not in ${sumsFile}`);
    process.exit(1);
  }
  return hash;
};
const url = (file) => `${base}/${file}`;
const folder = (file) => file.replace(/\.(zip|tar\.gz)$/, "");

const winX64 = archive("x86_64-pc-windows-msvc", "zip");
const winArm = archive("aarch64-pc-windows-msvc", "zip");

// Nothing is written until every manifest has been built, so a missing checksum leaves no
// half-written set behind.
const files = [];
const write = (dir, name, text) => files.push([dir, name, text]);

// ---- winget: three files, one per manifest type.
const wingetInstaller = (arch, file) => `- Architecture: ${arch}
  InstallerUrl: ${url(file)}
  InstallerSha256: ${sha(file).toUpperCase()}
  NestedInstallerFiles:
  - RelativeFilePath: ${folder(file)}\\gdt.exe
    PortableCommandAlias: gdt`;

write("winget", "Roxguel.Gdt.yaml", `# yaml-language-server: $schema=https://aka.ms/winget-manifest.version.1.6.0.schema.json
PackageIdentifier: Roxguel.Gdt
PackageVersion: ${version}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
`);

write("winget", "Roxguel.Gdt.locale.en-US.yaml", `# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.1.6.0.schema.json
PackageIdentifier: Roxguel.Gdt
PackageVersion: ${version}
PackageLocale: en-US
Publisher: Roxguel
PublisherUrl: https://github.com/roxguel
PublisherSupportUrl: https://github.com/${REPO}/issues
Author: Roxguel
PackageName: gdt
PackageUrl: https://gdt.roxguel.dev
License: MIT OR Apache-2.0
LicenseUrl: https://github.com/${REPO}/blob/main/LICENSE-MIT
Copyright: Copyright (c) 2026 Roxguel
ShortDescription: "${DESCRIPTION}"
Description: gdt installs and switches Godot builds (checked against their published checksums), manages addons from GitHub and the Godot stores with a lockfile, and runs GUT, gdUnit4 or script tests headless with CI-friendly exit codes. It is not affiliated with the Godot Foundation.
Moniker: gdt
Tags:
- godot
- gamedev
- version-manager
- cli
- addons
ReleaseNotesUrl: https://github.com/${REPO}/releases/tag/v${version}
Documentations:
- DocumentLabel: Documentation
  DocumentUrl: https://gdt.roxguel.dev
ManifestType: defaultLocale
ManifestVersion: 1.6.0
`);

write("winget", "Roxguel.Gdt.installer.yaml", `# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.1.6.0.schema.json
PackageIdentifier: Roxguel.Gdt
PackageVersion: ${version}
InstallerType: zip
NestedInstallerType: portable
Commands:
- gdt
ReleaseDate: ${date}
Installers:
${wingetInstaller("x64", winX64)}
${wingetInstaller("arm64", winArm)}
ManifestType: installer
ManifestVersion: 1.6.0
`);

// ---- Scoop: one JSON manifest, with the rules to update itself by hand-free tooling.
const scoopArch = (file) => ({ url: url(file), hash: sha(file), extract_dir: folder(file) });
const scoopAuto = (target) => ({
  url: `https://github.com/${REPO}/releases/download/v$version/gdt-v$version-${target}.zip`,
  extract_dir: `gdt-v$version-${target}`,
});
write(
  "scoop",
  "gdt.json",
  JSON.stringify(
    {
      version,
      description: DESCRIPTION,
      homepage: "https://gdt.roxguel.dev",
      license: "MIT|Apache-2.0",
      architecture: { "64bit": scoopArch(winX64), arm64: scoopArch(winArm) },
      bin: "gdt.exe",
      checkver: { github: `https://github.com/${REPO}` },
      autoupdate: {
        architecture: {
          "64bit": scoopAuto("x86_64-pc-windows-msvc"),
          arm64: scoopAuto("aarch64-pc-windows-msvc"),
        },
        hash: { url: `https://github.com/${REPO}/releases/download/v$version/SHA256SUMS` },
      },
    },
    null,
    4,
  ) + "\n",
);

// ---- Homebrew: one formula for macOS and Linux, Intel and arm64.
const brewSource = (target) => {
  const file = archive(target, "tar.gz");
  return `url "${url(file)}"\n      sha256 "${sha(file)}"`;
};
write("homebrew", "gdt.rb", `class Gdt < Formula
  desc "${BREW_DESCRIPTION}"
  homepage "https://gdt.roxguel.dev"
  license any_of: ["MIT", "Apache-2.0"]

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    on_arm do
      ${brewSource("aarch64-apple-darwin")}
    end
    on_intel do
      ${brewSource("x86_64-apple-darwin")}
    end
  end

  on_linux do
    on_arm do
      ${brewSource("aarch64-unknown-linux-musl")}
    end
    on_intel do
      ${brewSource("x86_64-unknown-linux-musl")}
    end
  end

  def install
    bin.install "gdt"
  end

  test do
    assert_match "gdt #{version}", shell_output("#{bin}/gdt --version")
  end
end
`);

for (const [dir, name, text] of files) {
  mkdirSync(join(outDir, dir), { recursive: true });
  writeFileSync(join(outDir, dir, name), text);
}
console.log(`Manifests for gdt ${version} written to ${outDir}`);
