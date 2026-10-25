class ActionsToolkit < Formula
  desc "Run GitHub Actions-style CI/CD workflows locally in Docker"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-aarch64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_AARCH64_SHA256"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-x86_64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_X86_64_SHA256"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-linux-x86_64.tar.gz"
    sha256 "REPLACE_WITH_LINUX_X86_64_SHA256"
  end

  def install
    bin.install "atk"
  end

  test do
    system "#{bin}/atk", "--help"
  end
end
