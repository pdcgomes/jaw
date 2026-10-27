# jaw's Homebrew formula, for Formula/jaw.rb in its tap. scripts/release.sh fills in url and sha256
# for a tagged release and prints the result.
class Jaw < Formula
  desc "WhatsApp in a terminal, fast, from the keyboard"
  homepage "https://github.com/pdcgomes/jaw"
  url "@URL@"
  sha256 "@SHA256@"
  license "MIT"
  head "https://github.com/pdcgomes/jaw.git", branch: "main"

  depends_on "go" => :build
  depends_on "rust" => :build
  # Text, and everything received, work without it; sending videos, GIFs and audio files needs it.
  depends_on "ffmpeg" => :recommended

  def install
    system "make", "install", "PREFIX=#{prefix}"
  end

  def caveats
    <<~EOS
      jaw starts jawd, the daemon that keeps your chats, when it isn't running. After an
      upgrade the jawd still running is the old one, and jaw says how to stop it.

      Without ffmpeg, text and everything you receive still work; sending videos, GIFs
      and audio files says it's missing, and `jawd status` lists it.
    EOS
  end

  test do
    said = shell_output("#{bin}/jaw --version")[/^jaw (\S+)$/, 1]
    assert_equal version.to_s, said unless version.head?
    assert_equal "jawd #{said}\n", shell_output("#{bin}/jawd --version")
    assert_match "jaw-engine #{said} (whatsmeow ", shell_output("#{libexec}/jaw/jaw-engine --version")

    # jawd on the demo engine's made-up account, in a folder of the test's own, and jaw drawing it:
    # no network, and nothing of yours.
    ENV["JAW_HOME"] = testpath/"jaw"
    jawd = spawn bin/"jawd", "--engine", "demo", "--no-notify", [:out, :err] => (testpath/"jawd.log").to_s
    begin
      frame = ""
      30.times do
        sleep 1
        frame = pipe_output("#{bin}/jaw snapshot --width 120 --height 30 2>&1")
        break if frame.include?("Lisbon Climbing")
      end
      assert_match "Lisbon Climbing", frame, (testpath/"jawd.log").read
      status = shell_output("#{bin}/jaw status")
      assert_match(/^jawd +#{Regexp.escape(said)} \(protocol /, status)
      refute_match "another build", status
    ensure
      Process.kill("TERM", jawd)
      Process.wait(jawd)
    end
  end
end
