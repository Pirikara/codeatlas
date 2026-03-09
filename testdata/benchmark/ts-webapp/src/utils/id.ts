export function generateId(): string {
  return Math.random().toString(36).substring(2, 15);
}

export function isValidId(id: string): boolean {
  return id.length > 0 && id.length <= 15;
}
