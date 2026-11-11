#!/usr/bin/env bash
# Genera dist/OpenCraftCutter-<versión>.dmg con OpenCraftCutter.app para Apple Silicon (con UNIVERSAL=1,
# también Intel; tarda el doble).
#
# Firma: con SIGN_ID="Developer ID Application: …" y NOTARY_PROFILE (perfil de
# `xcrun notarytool store-credentials`) la app se firma y se notariza, y se abre en cualquier Mac
# sin avisos. Sin ellos se firma «ad hoc»: funciona, pero la primera vez hay que abrirla con
# clic derecho › Abrir (o Ajustes del Sistema › Privacidad y seguridad › Abrir igualmente).
#
# Versión (MAYOR.MENOR.PARCHE, en [workspace.package] de Cargo.toml): sin BUMP se compila la
# versión actual. Con BUMP=patch|minor|major se sube antes de compilar y, si el build sale bien,
# se crea el commit «Versión X.Y.Z» y la etiqueta vX.Y.Z (requiere el árbol de trabajo limpio).
# El número de build (CFBundleVersion) es el número de commits: sube solo en cada commit.
set -euo pipefail
cd "$(dirname "$0")/.."

BUMP="${BUMP:-}"
if [ -n "$BUMP" ]; then
  case "$BUMP" in patch | minor | major) ;; *)
    echo "BUMP debe ser patch, minor o major" >&2
    exit 1
    ;;
  esac
  if [ -n "$(git status --porcelain)" ]; then
    echo "Hay cambios sin commit: haz commit antes de subir la versión." >&2
    exit 1
  fi
  OLD=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
  IFS=. read -r MA MI PA <<<"$OLD"
  case "$BUMP" in
    patch) PA=$((PA + 1)) ;;
    minor) MI=$((MI + 1)) PA=0 ;;
    major) MA=$((MA + 1)) MI=0 PA=0 ;;
  esac
  NEW="$MA.$MI.$PA"
  if git rev-parse -q --verify "refs/tags/v$NEW" >/dev/null; then
    echo "La etiqueta v$NEW ya existe." >&2
    exit 1
  fi
  echo "==> Versión $OLD → $NEW"
  # Solo la primera línea «version = …» (la del workspace).
  sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$NEW\"/" Cargo.toml
  # Si el build falla, se deshace el cambio de versión.
  trap 'git checkout -- Cargo.toml Cargo.lock 2>/dev/null; echo "Build fallido: versión sin cambiar." >&2' ERR
fi

NAME=OpenCraftCutter
BIN=opencraftcutter
BUNDLE_ID=com.sambotello.opencraftcutter
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BUILD=$(git rev-list --count HEAD 2>/dev/null || echo 0)
DIST=dist
APP="$DIST/$NAME.app"
DMG="$DIST/$NAME-$VERSION.dmg"
SIGN_ID="${SIGN_ID:--}"

echo "==> Compilando $NAME $VERSION (build $BUILD, release)"
TARGETS=(aarch64-apple-darwin)
[ "${UNIVERSAL:-0}" = 1 ] && TARGETS+=(x86_64-apple-darwin)
for t in "${TARGETS[@]}"; do
  rustup target list --installed | grep -qx "$t" || rustup target add "$t"
  MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release -p "$BIN" --target "$t"
done

echo "==> Creando $APP"
rm -rf "$DIST" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
BINS=()
for t in "${TARGETS[@]}"; do BINS+=("target/$t/release/$BIN"); done
lipo -create -output "$APP/Contents/MacOS/$BIN" "${BINS[@]}"

# Icono .icns a partir del icono que dibuja la propia app.
ICONSET="$DIST/$NAME.iconset"
mkdir -p "$ICONSET"
"target/aarch64-apple-darwin/release/$BIN" --export-icon "$DIST/icon-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$DIST/icon-1024.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$DIST/icon-1024.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/$NAME.icns"
rm -rf "$ICONSET" "$DIST/icon-1024.png"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>$NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleDevelopmentRegion</key><string>es</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Documento de OpenCraftCutter</string>
      <key>CFBundleTypeExtensions</key><array><string>opencraftcutter</string></array>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Owner</string>
    </dict>
  </array>
</dict>
</plist>
PLIST

echo "==> Firmando ($([ "$SIGN_ID" = - ] && echo 'ad hoc' || echo "$SIGN_ID"))"
codesign --force --options runtime --timestamp=none --sign "$SIGN_ID" "$APP" 2>/dev/null \
  || codesign --force --sign "$SIGN_ID" "$APP"
codesign --verify --strict "$APP"

echo "==> Creando $DMG"
STAGE="$DIST/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Aplicaciones"
hdiutil create -volname "$NAME $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
[ "$SIGN_ID" = - ] || codesign --force --sign "$SIGN_ID" "$DMG"

if [ -n "${NOTARY_PROFILE:-}" ] && [ "$SIGN_ID" != - ]; then
  echo "==> Notarizando"
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

if [ -n "$BUMP" ]; then
  trap - ERR
  git add Cargo.toml Cargo.lock
  git commit -q -m "Versión $VERSION"
  git tag "v$VERSION"
  echo "==> Commit «Versión ${VERSION}» y etiqueta v${VERSION} creados"
fi

echo "Listo: $DMG ($(du -h "$DMG" | cut -f1))"
