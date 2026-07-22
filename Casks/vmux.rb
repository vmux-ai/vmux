cask "vmux" do
  version "0.0.28"
  sha256 "69ba2264a570434042c71d8381bd07455661ed47ae9f5c208133b255c34cbb91"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.28/Vmux_0.0.28_aarch64.dmg"
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
