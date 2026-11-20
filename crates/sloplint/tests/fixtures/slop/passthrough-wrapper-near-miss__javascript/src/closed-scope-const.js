function wrap(s) {
  return format(s);
}
function main() {
  { const wrap = () => 1; } use(wrap);
}
