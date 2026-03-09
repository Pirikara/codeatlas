import { UserService } from "./user_service";

export interface Serializable {
    serialize(): string;
}

export class AdminService extends UserService implements Serializable {
    serialize(): string {
        return JSON.stringify(this);
    }

    async deleteUser(id: string): Promise<void> {
        // admin only
    }
}
