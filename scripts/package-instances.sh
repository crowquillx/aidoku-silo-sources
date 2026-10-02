#!/usr/bin/env bash
#
# Copy a packaged source into extra instances so Aidoku can connect to more
# than one server. Aidoku stores settings per source ID, so each copy has its
# own server URL, login, profile and cached session.
#
# Usage: scripts/package-instances.sh <package.aix> <count>
#
# Writes package-2.aix through package-<count>.aix next to the input. Instance
# N uses the ID `<id>N` and the name `<name> N`; the original stays as is so
# existing installs keep their settings and library.

set -euo pipefail

if [[ $# -ne 2 ]]; then
	echo "Usage: $0 <package.aix> <count>" >&2
	exit 2
fi

package=$1
count=$2

for ((n = 2; n <= count; n++)); do
	python3 - "$package" "${package%.aix}-$n.aix" "$n" <<'EOF'
import json
import sys
import zipfile

src, dst, n = sys.argv[1], sys.argv[2], sys.argv[3]
with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
    for item in zin.infolist():
        data = zin.read(item)
        if item.filename == "Payload/source.json":
            source = json.loads(data)
            info = source["info"]
            info["id"] = f"{info['id']}{n}"
            info["name"] = f"{info['name']} {n}"
            data = json.dumps(source, indent="\t").encode()
        zout.writestr(item, data)
EOF
done
