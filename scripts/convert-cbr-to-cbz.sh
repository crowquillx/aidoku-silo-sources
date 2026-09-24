#!/usr/bin/env bash
#
# Convert CBR (RAR) comic archives to CBZ (ZIP) so Silo can serve their pages
# to image-based clients like the Aidoku source. Run it against a manga/comic
# library directory, then rescan the library in Silo.
#
# Usage: scripts/convert-cbr-to-cbz.sh [--apply] [--delete] <directory>
#
# Requires one of `unar`, `unrar`, `7z`, or `7zz` to extract RAR, and `zip` to
# repack. Without `--apply` the script only reports what it would do.

set -euo pipefail

usage() {
	cat <<'EOF'
Convert CBR (RAR) comic archives to CBZ (ZIP).

Usage: convert-cbr-to-cbz.sh [options] <directory>

Options:
  -a, --apply    Perform the conversion (default is a dry run).
  -d, --delete   Delete the original .cbr after a successful conversion.
  -h, --help     Show this help.

Requires one of: unar, unrar, 7z, 7zz. Also requires: zip.
Converted files are written next to their originals with a .cbz extension.
EOF
}

apply=0
delete=0
directory=""
while [ $# -gt 0 ]; do
	case "$1" in
		-a | --apply) apply=1 ;;
		-d | --delete) delete=1 ;;
		-h | --help)
			usage
			exit 0
			;;
		-*)
			echo "Unknown option: $1" >&2
			usage >&2
			exit 2
			;;
		*) directory="$1" ;;
	esac
	shift
done

if [ -z "$directory" ] || [ ! -d "$directory" ]; then
	echo "Provide an existing directory to scan." >&2
	usage >&2
	exit 2
fi

find_extractor() {
	for candidate in unar unrar 7z 7zz; do
		if command -v "$candidate" >/dev/null 2>&1; then
			echo "$candidate"
			return
		fi
	done
	echo ""
}

extractor="$(find_extractor)"
zipper="$(command -v zip || true)"

found=0
converted=0
failed=0

while IFS= read -r -d '' file; do
	found=$((found + 1))
	file_abs="$(cd "$(dirname "$file")" && pwd)/$(basename "$file")"
	out_abs="${file_abs%.*}.cbz"

	if [ -e "$out_abs" ]; then
		echo "skip (cbz exists): $out_abs"
		continue
	fi

	if [ "$apply" -eq 0 ]; then
		echo "would convert: $file_abs -> $out_abs"
		continue
	fi

	if [ -z "$extractor" ] || [ -z "$zipper" ]; then
		echo "error: need an extractor (unar/unrar/7z) and zip to convert $file_abs" >&2
		failed=$((failed + 1))
		continue
	fi

	# One extraction directory per archive, removed before the next one. The
	# trap only covers an interrupted run.
	tmp="$(mktemp -d)"
	trap 'rm -rf "$tmp"' EXIT

	ok=1
	case "$extractor" in
		unar) unar -q -o "$tmp" "$file_abs" >/dev/null 2>&1 || ok=0 ;;
		unrar) unrar x -idq "$file_abs" "$tmp/" >/dev/null 2>&1 || ok=0 ;;
		7z | 7zz) "$extractor" x -y -o"$tmp" "$file_abs" >/dev/null 2>&1 || ok=0 ;;
	esac

	if [ "$ok" -eq 1 ]; then
		(cd "$tmp" && "$zipper" -q -r -X "$out_abs" .) >/dev/null 2>&1 || ok=0
	fi

	if [ "$ok" -eq 1 ] && [ -e "$out_abs" ]; then
		echo "converted: $file_abs -> $out_abs"
		converted=$((converted + 1))
		if [ "$delete" -eq 1 ]; then
			rm -f "$file_abs"
		fi
	else
		echo "failed: $file_abs" >&2
		rm -f "$out_abs"
		failed=$((failed + 1))
	fi
	rm -rf "$tmp"
done < <(find "$directory" -type f -iname '*.cbr' -print0)

echo "found=$found converted=$converted failed=$failed"
[ "$failed" -eq 0 ]
