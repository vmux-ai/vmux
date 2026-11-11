import argparse
import base64
import concurrent.futures
import hashlib
import io
import json
import re
import subprocess
import tarfile
import xml.etree.ElementTree as ET
from pathlib import Path


class LogoCatalog:
    def __init__(self, root):
        self.directory = root / "crates/util/vmux_ui/assets/acp-logos"
        self.path = self.directory / "catalog.json"
        self.catalog = json.loads(self.path.read_text())

    @staticmethod
    def fetch(url):
        if not url.startswith("https://"):
            raise ValueError("Logo sources must use HTTPS")
        return subprocess.check_output(["curl", "-fsSL", url])

    @staticmethod
    def validate(svg):
        root = ET.fromstring(svg)
        if root.tag != "{http://www.w3.org/2000/svg}svg":
            raise ValueError("Logo must be an SVG")
        for element in root.iter():
            if element.tag.rsplit("}", 1)[-1] in ("script", "foreignObject", "iframe"):
                raise ValueError("Logo contains active content")
            for key, value in element.attrib.items():
                if key.lower().startswith("on"):
                    raise ValueError("Logo contains an event handler")
                if key.rsplit("}", 1)[-1] in ("href", "src") and not value.startswith("#"):
                    raise ValueError("Logo references an external resource")

    def registry_icon(self, agent):
        url = agent["icon"]
        if not url.startswith("https://cdn.agentclientprotocol.com/registry/"):
            raise ValueError("Registry logo must use the official CDN")
        return agent["id"], self.fetch(url).decode()

    def sync(self, version):
        package = self.catalog["package"]
        metadata = json.loads(self.fetch(f"https://registry.npmjs.org/{package['name']}/{version}"))
        archive = self.fetch(metadata["dist"]["tarball"])
        integrity = "sha512-" + base64.b64encode(hashlib.sha512(archive).digest()).decode()
        if integrity != metadata["dist"]["integrity"]:
            raise ValueError("Logo package integrity mismatch")
        if version == package["version"] and integrity != package["integrity"]:
            raise ValueError("Pinned logo package has changed")
        agents = json.loads(self.fetch(self.catalog["registry"]))["agents"]
        existing = {logo["id"]: logo for logo in self.catalog["logos"]}
        remote = [agent for agent in agents if existing.get(agent["id"], {}).get("source") != "lobe-icons"]
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            registry = dict(pool.map(self.registry_icon, remote))
        logos = []
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            for agent in agents:
                previous = existing.get(agent["id"], {})
                if previous.get("source") == "lobe-icons":
                    asset = previous["asset"]
                    if not asset.startswith("icons/") or ".." in Path(asset).parts:
                        raise ValueError("Unsafe logo package asset")
                    member = tar.getmember("package/" + asset)
                    if not member.isreg():
                        raise ValueError("Logo package asset must be a regular file")
                    svg = tar.extractfile(member).read().decode()
                    source, license_name = "lobe-icons", "MIT"
                else:
                    asset = agent["icon"]
                    svg = registry[agent["id"]]
                    source, license_name = "acp-registry", "Apache-2.0"
                self.validate(svg)
                colors = sorted(set(re.findall(r"#[0-9A-Fa-f]{3,8}(?![0-9A-Fa-f])", svg)))
                colors = [color for color in colors if len(color) in (4, 5, 7, 9)]
                color_mode = "mixed" if colors and "currentColor" in svg else "color" if colors else "monochrome"
                logos.append({"id": agent["id"], "name": agent["name"], "aliases": previous.get("aliases", []), "source": source, "asset": asset, "license": license_name, "color_mode": color_mode, "colors": colors, "svg": svg})
        result = dict(self.catalog)
        result["package"] = dict(package, version=metadata["version"], integrity=integrity)
        result["logos"] = logos
        temporary = self.path.with_suffix(".tmp")
        temporary.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        temporary.replace(self.path)
        print(f"Updated {len(logos)} ACP logos from {package['name']} {metadata['version']}")


if __name__ == "__main__":
    catalog = LogoCatalog(Path(__file__).resolve().parents[1])
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default=catalog.catalog["package"]["version"])
    options = parser.parse_args()
    catalog.sync(options.version)
