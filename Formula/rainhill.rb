# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.0.0"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.0/rail-v1.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "13bc04bd6748ac4414dc1b24b6323f65e830d67924aaa786863c77d2a75a6077"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.0/rail-v1.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "c446ae2c6d3871c178bd1978986a37ecd8ece47024a6731933310aee8ab3f085"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.0/rail-v1.0.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "adebc0832aca43484664a92266576b0f68c0c3a76271dfa08f28776df83c358d"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.0/rail-v1.0.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "1428f3a35b20619534b29a38ff63172e9e2374425bc803a2d07aa97811262786"
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
