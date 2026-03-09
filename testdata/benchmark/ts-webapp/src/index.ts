import { createApp } from "./app";
import { UserController } from "./controllers/user.controller";
import { UserService } from "./services/user.service";
import { UserRepository } from "./repositories/user.repository";

const PORT = process.env.PORT || 3000;

export function main(): void {
  const repository = new UserRepository();
  const service = new UserService(repository);
  const controller = new UserController(service);

  const app = createApp(controller);
  app.listen(PORT, () => {
    console.log(`Server running on port ${PORT}`);
  });
}

main();
