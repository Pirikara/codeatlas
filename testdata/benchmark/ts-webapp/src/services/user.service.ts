import { BaseService } from "./base.service";
import { User, CreateUserInput } from "../types";
import { generateId } from "../utils/id";
import { validateEmail } from "../utils/validation";

export class UserService extends BaseService<User> {
  async create(input: CreateUserInput): Promise<User> {
    if (!validateEmail(input.email)) {
      throw new Error("Invalid email");
    }

    const user: User = {
      id: generateId(),
      name: input.name,
      email: input.email,
      role: input.role,
      createdAt: new Date(),
      updatedAt: new Date(),
    };

    if (!this.validate(user)) {
      throw new Error("Validation failed");
    }

    return this.save(user);
  }

  async findByEmail(email: string): Promise<User | null> {
    const all = await this.findAll();
    return all.find((u) => u.email === email) ?? null;
  }
}
