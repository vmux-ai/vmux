cask "vmux" do
  version "0.0.17"
  sha256 "8d42328254755fbc034bc2d5bd5ffe565ec86d0d68e5552a1b240abeafa9f33a"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.17/Vmux_0.0.17_aarch64.dmg"
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
