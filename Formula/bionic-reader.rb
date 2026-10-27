class BionicReader < Formula
  desc "Local word-prefix emphasis for text streams and terminal sessions"
  homepage "https://github.com/arturoaviles/bionic-reader"
  license "MIT"
  head "https://github.com/arturoaviles/bionic-reader.git", branch: "main"

  depends_on "rust" => :build

  def fetch
    fetch_args = respond_to?(:std_cargo_fetch_args) ? std_cargo_fetch_args : ["--locked"]
    system "cargo", "fetch", *fetch_args
  end

  def install
    system "cargo", "install", *std_cargo_args
    generate_completions_from_executable(bin/"bionic-reader", "completions")
    pkgshare.install "integrations", "completions", "packaging/config.example.toml"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_match "bionic-reader", shell_output("#{bin}/bionic-reader --version")
    assert_equal "reading", shell_output("#{bin}/bionic-reader --style never -t reading")
    assert_equal "\e[1mread\e[22ming", shell_output("#{bin}/bionic-reader --style always -t reading")
  end
end
