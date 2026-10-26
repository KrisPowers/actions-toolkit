class Atk < Formula
  desc "Run GitHub Actions-style CI/CD workflows locally in Docker"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "1.0.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-aarch64.tar.gz"
      sha256 "25467f3ee1b234affe6172f098bebf54705d26cc4019bcef860b2b833f553801"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-x86_64.tar.gz"
      sha256 "8f121dd052af6e95a19f51b9bb50994642773f13ea836b27f6cd1c8b03f4167b"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-linux-x86_64.tar.gz"
    sha256 "921ad6c9d299f8958bd2a93274760b2c012bc334f0e6c7e57613451954c05aeb"
  end

  def install
    bin.install "atk"
  end

  test do
    system "#{bin}/atk", "--help"
  end
end
