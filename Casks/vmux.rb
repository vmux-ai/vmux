cask "vmux" do
  version "0.0.25"
  sha256 "3c0fb73ff83260d9f9bea87adac1d365e14964d23d520ed7ea8deb4f9664cd30"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.25/Vmux_0.0.25_aarch64.dmg"
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
