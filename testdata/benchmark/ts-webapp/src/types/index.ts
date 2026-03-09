export interface Entity {
  id: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface Repository<T extends Entity> {
  findById(id: string): Promise<T | null>;
  findAll(): Promise<T[]>;
  save(entity: T): Promise<T>;
  delete(id: string): Promise<void>;
}

export type UserRole = "admin" | "member" | "guest";

export enum HttpStatus {
  OK = 200,
  Created = 201,
  BadRequest = 400,
  Unauthorized = 401,
  NotFound = 404,
}

export interface Request {
  path: string;
  params?: Record<string, string>;
  body?: unknown;
  headers?: Record<string, string>;
}

export interface Response {
  status(code: number): Response;
  json(data: unknown): void;
}

export type Middleware = (req: Request, res: Response, next: () => void) => void;

export interface CreateUserInput {
  name: string;
  email: string;
  role: UserRole;
}

export interface User extends Entity {
  name: string;
  email: string;
  role: UserRole;
}
