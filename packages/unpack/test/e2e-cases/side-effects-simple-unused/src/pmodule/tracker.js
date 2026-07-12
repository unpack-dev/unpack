export const log = [];

export function track(file) {
  log.push(file);
  log.sort();
}
