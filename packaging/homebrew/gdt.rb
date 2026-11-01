class Gdt < Formula
  desc "Godot Development Tools: version manager, addon manager and test runner for Godot"
  homepage "https://gdt.roxguel.dev"
  version "0.1.0"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/roxguel/godot-dev-tools/releases/download/v0.1.0/gdt-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "1cf504734876973ee9dbd629515110e376432728dcdd2a48baf55ac7e32d0612"
    end
    on_intel do
      url "https://github.com/roxguel/godot-dev-tools/releases/download/v0.1.0/gdt-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "34a01f7944818a7853d6aafc4873902195d0d7a5105b54f1ade8c46a55ea88b6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/roxguel/godot-dev-tools/releases/download/v0.1.0/gdt-v0.1.0-aarch64-unknown-linux-musl.tar.gz"
      sha256 "a677181030c5690f8c0ababbde7ae9f81cadb29c7aa8e2e667727d8b3cb4a1f6"
    end
    on_intel do
      url "https://github.com/roxguel/godot-dev-tools/releases/download/v0.1.0/gdt-v0.1.0-x86_64-unknown-linux-musl.tar.gz"
      sha256 "8866aa18ac2404711835cb031a568499ff4e65eb16d06f6e3ee06eacf241204e"
    end
  end

  livecheck do
    url :stable
    strategy :github_latest
  end

  def install
    bin.install "gdt"
  end

  test do
    assert_match "gdt #{version}", shell_output("#{bin}/gdt --version")
  end
end
