class Atk < Formula
  desc "Run GitHub Actions-style CI/CD workflows locally in Docker"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "1.0.1"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-aarch64.tar.gz"
      sha256 "99fd7cc1a69b6af6ac59d8f8685f12203b536b343623c3693e95c20bfabc584b"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-x86_64.tar.gz"
      sha256 "9b2baa4e803018587aee7dc5e50311c1ae63dfa0f0509289f65a0343afe44c14"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-linux-x86_64.tar.gz"
    sha256 "68cca936bacef4212cf195cc8d8a2c7920c6383a817510dcc0f345f178c1a561"
  end

  def install
    bin.install "atk"
  end

  test do
    system "#{bin}/atk", "--help"
  end
end
