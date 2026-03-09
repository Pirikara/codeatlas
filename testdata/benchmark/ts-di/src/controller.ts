import { UserService } from './user_service';

export class Controller {
  constructor(private service: UserService) {}
  create() { this.service.save('test'); }
  get(id: string) { return this.service.find(id); }
}
