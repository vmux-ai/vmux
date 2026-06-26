cask "vmux" do
  version "0.0.19"
  sha256 "5a7465a7c385575fd42157628d0045eddba561666d61a58a2576e836920cf831"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.19/Vmux_0.0.19_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai/"

  depends_on macos: :ventura

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
