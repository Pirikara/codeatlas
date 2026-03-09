export interface User {
    name: string;
    password: string;
}

export class UserRepository {
    private users: User[] = [];

    async save(user: User): Promise<void> {
        this.users.push(user);
    }

    async findById(id: string): Promise<User | undefined> {
        return this.users.find(u => u.name === id);
    }
}
