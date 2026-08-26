cask "vmux" do
  version "0.0.31"
  sha256 "a191bc1fd233eef4e2c67ec2a57ec28c0d3f1237abbf1fe9d17d560eff5b8605"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.31/Vmux_0.0.31_aarch64.dmg"
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
