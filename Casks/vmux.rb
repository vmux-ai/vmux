cask "vmux" do
  version "0.0.16"
  sha256 "9473cd939a92c274edf5995dc47e59a7a2a9dab82320ee446bac1015a3ba956b"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.16/Vmux_0.0.16_aarch64.dmg"
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
