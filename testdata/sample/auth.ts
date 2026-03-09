export interface AuthConfig {
    secret: string;
    expiresIn: number;
}

export class AuthService {
    constructor(private config: AuthConfig) {}

    async validateToken(token: string): Promise<boolean> {
        return verify(token, this.config.secret);
    }

    async createToken(userId: string): Promise<string> {
        return sign({ userId }, this.config.secret);
    }
}

export function hashPassword(password: string): string {
    return bcrypt.hash(password, 10);
}

export type UserId = string;

export enum Role {
    Admin = "admin",
    User = "user",
}
