#!/usr/bin/env bash
# Genera docs/guia/OpenCraftCutter-Guia.pdf a partir de docs/guia/guia.html con Chrome sin ventana
# (con el índice del PDF a partir de los títulos).
set -euo pipefail
cd "$(dirname "$0")/../docs/guia"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
"$CHROME" --headless=new --disable-gpu --no-pdf-header-footer --generate-pdf-document-outline \
  --print-to-pdf="$PWD/OpenCraftCutter-Guia.pdf" "file://$PWD/guia.html" 2>/dev/null
echo "Listo: docs/guia/OpenCraftCutter-Guia.pdf"
