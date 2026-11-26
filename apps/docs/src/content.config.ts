import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";
import { z } from "astro/zod";

import { defineCollection } from "astro:content";

export const collections = {
	docs: defineCollection({
		loader: docsLoader(),
		schema: docsSchema({
			extend: z.object({
				/** What a generated page is built from, shown under its title; set only by `cargo xtask docs`. */
				generated: z.string().optional(),
			}),
		}),
	}),
};
