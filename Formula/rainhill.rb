# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.1.2"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.2/rail-v1.1.2-aarch64-apple-darwin.tar.gz"
      sha256 "3369024000a32701961455c824057bad927fe69759f3c527a03f76b8c4012dc7"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.2/rail-v1.1.2-x86_64-apple-darwin.tar.gz"
      sha256 "7c4c7d06ab354dafcf63b94e8f863fdb4e967ac05cb662f4934546c17fbc60be"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.2/rail-v1.1.2-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "55845d7e061e84de8a2f82a2a8426196516550c10c881e155a386771e0593973"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.1.2/rail-v1.1.2-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f457254d04b67483866bb5b5889214c860ae9ca69855e990e1f0b44b03839e2b"
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
