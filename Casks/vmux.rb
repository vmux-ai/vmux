cask "vmux" do
  version "0.0.13"
  sha256 "b6b339890b74f4693e36674c3b6b14228d0af722d5cb2c940646fcf363854e16"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.13/Vmux_0.0.13_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai"

  depends_on macos: ">= :ventura"

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
