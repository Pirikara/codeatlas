import { Entity, Repository } from "../types";

export abstract class BaseService<T extends Entity> {
  constructor(protected repository: Repository<T>) {}

  async findById(id: string): Promise<T | null> {
    return this.repository.findById(id);
  }

  async findAll(): Promise<T[]> {
    return this.repository.findAll();
  }

  async save(entity: T): Promise<T> {
    return this.repository.save(entity);
  }

  async delete(id: string): Promise<void> {
    return this.repository.delete(id);
  }

  protected validate(entity: T): boolean {
    return entity.id !== undefined && entity.id.length > 0;
  }
}
