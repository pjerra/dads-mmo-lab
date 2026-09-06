set -u
cd "${1:-.}"   # run from a checkout of lane/b41
BASE=cfb4c04f
TIP=$(git rev-parse HEAD)
echo "BASE=$BASE TIP=$TIP (working tree used for tip content)"
files=$(git ls-files 'pyplan/*.md' 'pyplan/**/*.md')
for md in $files; do
  grep -oE '(networking|native|controller_view)\.py:[0-9]+' "$md" | sort -u | while read -r cite; do
    f=${cite%%:*}; n=${cite##*:}
    case $f in
      networking.py) p=pylauncher/yulon/networking.py;;
      native.py) p=pylauncher/yulon/catalog/native.py;;
      controller_view.py) p=pylauncher/yulon/ui/controller_view.py;;
    esac
    b=$(git show $BASE:$p | sed -n "${n}p")
    t=$(sed -n "${n}p" "$p")
    if [ "$b" != "$t" ]; then
      echo "MOVED  $md  $cite"
      echo "   base: $b"
      echo "   tip : $t"
    fi
  done
done
