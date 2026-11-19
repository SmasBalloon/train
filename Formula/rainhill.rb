# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.1.1"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.1/rail-v1.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "b50bb7b1020fd20a3b0d2a3660ef5c1a3d2d86f5c2de370e832188b375f8a2b2"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.1/rail-v1.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "0becd2bce44deba595d2a6b6911af523b67f8e9275c18bdcf885c2ae2510c80b"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.1/rail-v1.1.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "5b53fa92eae0ef8c5bb0fbec1b95479b96518f5a932c47befc9c4dc113fb3113"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.1/rail-v1.1.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "dcb0e8b3f4e497774d3b9607e862340a8762071f212954b3c422b4d60d41fe2d"
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
