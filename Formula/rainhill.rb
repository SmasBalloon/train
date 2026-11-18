# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.1.0"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.0/rail-v1.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "5fba5c889648282933f2ee3feb58715de8f6046ce3bf5367f31eb63a0a9e51e2"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.0/rail-v1.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "e9ffcea8e56804e8fa469ae8fea2d1a50944ddf09cf2491b5d3dcd08a71db880"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.0/rail-v1.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a5263e0c846468b9b8bf6a137413a532d49f33e88b17227ca8a14f6e8bf8e04d"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.0/rail-v1.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f5a7242a3b6a9ea69ba2fc0b3528909c6c977e320ec753635d8e7ec9d68ca323"
    end
  end

  def install
    bin.install "rail"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/rail --version")
  end
end
