cask "vmux" do
  version "0.0.27"
  sha256 "673d1aea2df23a45dc1d8bf807ad899b4925abad759a2433f226052a9fa3b28d"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.27/Vmux_0.0.27_aarch64.dmg"
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
