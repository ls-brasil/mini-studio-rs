"""Reúne metadados e avisos das dependências locais usadas no pacote."""
import json
import subprocess
from pathlib import Path

repo = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version", "1",
     "--filter-platform", "x86_64-pc-windows-msvc"], cwd=repo
))
resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
parts = ["MINI STUDIO — AVISOS DE TERCEIROS\n\n"
         "Os componentes abaixo mantêm as licenças dos respectivos autores.\n"
         "As fontes Segoe UI são lidas do Windows e não são redistribuídas.\n"]
parts.append("\n" + "=" * 72 + "\n" +
             (repo / "docs" / "NDI-headers-LICENSE.txt").read_text(encoding="utf-8"))
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["name"] == "mini-studio-rs" or package["id"] not in resolved:
        continue
    root = Path(package["manifest_path"]).parent
    parts.append("\n" + "=" * 72 + f"\n{package['name']} {package['version']}\n"
                 f"Licença declarada: {package.get('license') or 'ver arquivo de licença'}\n"
                 f"Origem: {package.get('repository') or package.get('homepage') or package['source']}\n")
    files = set()
    for pattern in ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "license*", "licenses/*"):
        files.update(p for p in root.glob(pattern) if p.is_file())
    if package.get("license_file"):
        files.add(root / package["license_file"])
    for file in sorted(files):
        if file.exists() and file.stat().st_size < 1_000_000:
            parts.append(f"\n--- {file.relative_to(root)} ---\n" + file.read_text(encoding="utf-8", errors="replace"))
output = repo / "target" / "Avisos-de-terceiros.txt"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text("\n".join(parts), encoding="utf-8-sig")
print(output)
