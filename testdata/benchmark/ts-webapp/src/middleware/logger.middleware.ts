import { Middleware } from "../types";

export const loggerMiddleware: Middleware = (req, _res, next) => {
  const timestamp = formatTimestamp(new Date());
  console.log(`[${timestamp}] ${req.path}`);
  next();
};

function formatTimestamp(date: Date): string {
  return date.toISOString();
}
