cask "vmux" do
  version "0.0.26"
  sha256 "67f53918e3e527b7b85e5ef33d2208b55234b6b73ff39f3794c6cd58a3c667bc"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.26/Vmux_0.0.26_aarch64.dmg"
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
