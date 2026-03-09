import { UserController } from "./controllers/user.controller";
import { authMiddleware } from "./middleware/auth.middleware";
import { loggerMiddleware } from "./middleware/logger.middleware";

interface App {
  listen(port: number | string, callback: () => void): void;
}

export function createApp(controller: UserController): App {
  const middlewares = [loggerMiddleware, authMiddleware];

  return {
    listen(port: number | string, callback: () => void) {
      for (const mw of middlewares) {
        mw({ path: "/" }, {}, () => {});
      }
      controller.registerRoutes();
      callback();
    },
  };
}
