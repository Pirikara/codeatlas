import { Middleware } from "../types";

const TOKEN_HEADER = "x-auth-token";

export const authMiddleware: Middleware = (req, res, next) => {
  const token = req.headers?.[TOKEN_HEADER];
  if (!token) {
    res.status(401).json({ error: "Unauthorized" });
    return;
  }
  if (!verifyToken(token)) {
    res.status(401).json({ error: "Invalid token" });
    return;
  }
  next();
};

function verifyToken(token: string): boolean {
  return token.length > 0;
}
