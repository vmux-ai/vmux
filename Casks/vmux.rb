cask "vmux" do
  version "0.0.23"
  sha256 "f581e688c429432ee077caa39e95924eefc615f17834e592ffbd4d1944644086"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.23/Vmux_0.0.23_aarch64.dmg"
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
