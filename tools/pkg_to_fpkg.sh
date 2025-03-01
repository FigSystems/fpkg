#!/bin/bash

[ -n "$1" ] || {
  echo "Please specify the file you would like me to convert :)"
  exit 1
}
[ -f "$1" ] || {
  echo "File $1 does not exist!"
  exit 1
}

old_pwd="$(pwd)"
mkdir tmp
cd tmp
tar -xf "../$1"

grep -q -E "pkgname ?= ?.*" .PKGINFO || {
  echo "Invalid package! Name is not specified!"
  exit 2
}
grep -q -E "pkgver ?= ?.*" .PKGINFO || {
  echo "Invalid package! Version is not specified"
  exit 2
}

version="$(cat .PKGINFO | grep -P -o "(?<=pkgver ?=) ?.*" | grep -E -o "([0-9]+\.)+[0-9]+")"
[ -n "$version" ] || {
  echo "Version not defined in .PKGINFO!"
  exit 2
}

name="$(cat .PKGINFO | grep -P -o "(?<=pkgname ?=) ?.*" | awk '{$1=$1};1')"
[ -n "$name" ] || {
  echo "Name not defined in .PKGINFO!"
  exit 2
}

echo "Package: $name-$version"

out="$old_pwd/$name-$version"
rm -rf "$out"
mkdir -p "$out" "$out/fpkg"

cat <<EOF >>"$out/fpkg/pkg.kdl"
name "$name"
version "$version"

EOF

while read p; do
  if echo "$p" | grep -q -E "^depend ?="; then
    echo -n ""
  else
    continue
  fi

  depend_name="$(echo -n "$p" | grep -P -o "(?<=depend ?= ?)[a-zA-Z\-_]+")"
  depend_versioning=$(echo -n "$p" | grep -P -o "(?<=depend ?= ?${depend_name}).*")
  echo -n "depends \"$depend_name\"" >>"$out/fpkg/pkg.kdl"
  if [ -n "$depend_versioning" ]; then
    echo -n " version=\"$depend_versioning\"" >>"$out/fpkg/pkg.kdl"
  fi
  echo >>"$out/fpkg/pkg.kdl"
done <".PKGINFO"

rm -f .PKGINFO .INSTALL .BUILD .MTREE
cp -r ./* "$out/"

cd ..
rm -rf tmp

fpkg gen-pkg "$out"
rm -rf "$out"

exit 0
