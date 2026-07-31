cask "vmux" do
  version "0.0.30"
  sha256 "eca282cdc344b6349c7c8eb17949ab4734b7fc2dfdc491dcb3205605bcfc6626"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.30/Vmux_0.0.30_aarch64.dmg"
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
