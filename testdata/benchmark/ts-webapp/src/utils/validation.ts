export function validateEmail(email: string): boolean {
  return email.includes("@") && email.includes(".");
}

export function validateRequired(value: unknown): boolean {
  return value !== null && value !== undefined;
}
