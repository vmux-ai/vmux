cask "vmux" do
  version "0.0.20"
  sha256 "1aee4b1ffe3da77a0f519da0d3e80b01cd18d835ab09957e1897354d509e7ae2"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.20/Vmux_0.0.20_aarch64.dmg"
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
