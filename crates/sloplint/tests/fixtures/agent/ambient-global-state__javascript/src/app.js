let counter = 0;

export function bump() {
  counter += 1;
  return counter;
}

export function stash() {
  process.env.TOKEN = 'x';
  process.env['TOKEN'] = 'y';
}
