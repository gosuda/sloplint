const limit = 10;

config.env.TOKEN = 'x';

export function bump(counter) {
  return counter + limit;
}
