class Doer < Formula
  desc "Vim-flavoured terminal todo app"
  homepage "https://github.com/armanarutiunov/doer"
  url "https://github.com/armanarutiunov/doer/archive/refs/tags/v0.3.0.tar.gz"
  sha256 "REPLACE_WITH_SHA256_OF_THE_SOURCE_TARBALL"
  license "MIT"
  head "https://github.com/armanarutiunov/doer.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "cli")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/doer --version")

    require "pty"
    ENV["DOER_HOME"] = testpath/"doer-home"
    PTY.spawn("#{bin}/doer") do |_reader, writer, pid|
      writer.write "q"
      Process.wait pid
    rescue Errno::EIO
      nil
    end
    assert_predicate testpath/"doer-home", :exist?
  end
end
