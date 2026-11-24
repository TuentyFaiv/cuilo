<script lang="ts">
	import type { HTMLFieldsetAttributes } from "svelte/elements";

	interface Props extends Omit<HTMLFieldsetAttributes, "aria-describedby" | "aria-labelledby" | "role"> {
		/** Names the group for assistive technology. */
		label: string;
		/** Extra guidance shown under the label. */
		description?: string;
	}

	let { label, description, class: className, children, ...rest }: Props = $props();
	const id = $props.id();

	// @cuilo if stack.tailwind
	const classes = {
		root: "grid min-w-0 gap-2",
		label: "text-sm font-semibold",
		description: "text-sm text-zinc-500",
		content: "flex flex-wrap items-center gap-2",
	};
	// @cuilo else
	const classes = { root: "root", label: "label", description: "description", content: "content" };
	// @cuilo end
</script>

<fieldset
	{...rest}
	aria-labelledby="{id}-label"
	aria-describedby={description ? `${id}-description` : undefined}
	class={[classes.root, className]}
>
	<span id="{id}-label" class={classes.label}>{label}</span>
	{#if description}
		<p id="{id}-description" class={classes.description}>{description}</p>
	{/if}
	<div class={classes.content}>
		{@render children?.()}
	</div>
</fieldset>
<!-- @cuilo if !stack.tailwind -->

<style>
	.root {
		display: grid;
		gap: 0.5rem;
		min-inline-size: 0;
		margin: 0;
		border: 0;
		padding: 0;
	}

	.label {
		font-size: 0.875rem;
		font-weight: 600;
	}

	.description {
		margin: 0;
		font-size: 0.875rem;
		color: var(--ui-muted-foreground, #71717a);
	}

	.content {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}
</style>
<!-- @cuilo end -->
