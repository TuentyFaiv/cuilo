import { drizzle } from "drizzle-orm/node-postgres";

const url = process.env.DATABASE_URL;
if (!url) {
	throw new Error("DATABASE_URL is not set.");
}

/** The Drizzle database for the PostgreSQL server at `DATABASE_URL`. */
export const db = drizzle(url);
