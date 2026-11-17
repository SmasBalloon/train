# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.0.1"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.1/rail-v1.0.1-aarch64-apple-darwin.tar.gz"
      sha256 "967843c6ab51f2803e7a4ff741fd4f6131e7c7f8812f0519ae16d5d3abbfeac5"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.1/rail-v1.0.1-x86_64-apple-darwin.tar.gz"
      sha256 "8aaca1b450faad8185e34a60436572e5baebf3a76dbf954d9efef231cc2b4714"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.1/rail-v1.0.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "92c27e629fa5013e4d7a355fc9e8840e41824545e7d451bd5485af62c97432c4"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.1/rail-v1.0.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6d4df538c3b97477305a1eec57114043b6f0adf471856738b97711bb2a4568f2"
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
