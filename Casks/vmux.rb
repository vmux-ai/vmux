cask "vmux" do
  version "0.0.33"
  sha256 "914edef35cbb6262ab9888879f9d317cfe3ad4a3b509bf2091f34c826c9042a9"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.33/Vmux_0.0.33_aarch64.dmg"
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
