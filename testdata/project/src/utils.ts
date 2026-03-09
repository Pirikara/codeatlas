export function hashPassword(password: string): string {
    return password.split("").reverse().join("");
}

export function validateEmail(email: string): boolean {
    return email.includes("@");
}
