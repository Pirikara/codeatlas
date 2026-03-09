import { UserRepository } from "./user_repository";
import { hashPassword } from "./utils";

export class UserService {
    constructor(private repo: UserRepository) {}

    async createUser(name: string, password: string): Promise<void> {
        const hashed = hashPassword(password);
        await this.repo.save({ name, password: hashed });
    }

    async findUser(id: string) {
        return this.repo.findById(id);
    }
}
