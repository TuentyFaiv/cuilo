<script lang="ts">
	let { title }: { title: string } = $props();
</script>

<h2 class="my-example">{title}</h2>
<!-- @cuilo if styled -->

<style>
	.my-example {
		display: block;
	}
</style>
<!-- @cuilo end -->
