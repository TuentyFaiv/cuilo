import { PrismaPg } from "@prisma/adapter-pg";

import { PrismaClient } from "../generated/prisma/client.js"; // @cuilo if stack.imports-js
import { PrismaClient } from "../generated/prisma/client.ts"; // @cuilo if stack.imports-ts
import { PrismaClient } from "../generated/prisma/client"; // @cuilo if !stack.imports-js && !stack.imports-ts

const url = process.env.DATABASE_URL;
if (!url) {
	throw new Error("DATABASE_URL is not set.");
}

const shared = globalThis as typeof globalThis & { prisma?: PrismaClient };
shared.prisma ??= new PrismaClient({ adapter: new PrismaPg({ connectionString: url }) });

/** The Prisma client for the PostgreSQL server at `DATABASE_URL`, created once and kept across hot reloads. */
export const prisma = shared.prisma;

export type { PrismaClient };
