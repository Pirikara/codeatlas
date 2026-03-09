import { UserService } from "../services/user.service";
import { CreateUserInput, HttpStatus } from "../types";

export class UserController {
  constructor(private service: UserService) {}

  registerRoutes(): void {
    // route registration
  }

  async create(input: CreateUserInput): Promise<{ status: number; data: unknown }> {
    const user = await this.service.create(input);
    return { status: HttpStatus.Created, data: user };
  }

  async getById(id: string): Promise<{ status: number; data: unknown }> {
    const user = await this.service.findById(id);
    if (!user) {
      return { status: HttpStatus.NotFound, data: null };
    }
    return { status: HttpStatus.OK, data: user };
  }

  async list(): Promise<{ status: number; data: unknown }> {
    const users = await this.service.findAll();
    return { status: HttpStatus.OK, data: users };
  }

  async remove(id: string): Promise<{ status: number; data: unknown }> {
    await this.service.delete(id);
    return { status: HttpStatus.OK, data: null };
  }
}
