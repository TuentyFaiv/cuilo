import styles from "./my-example.module.css"; // @cuilo if styled

export interface MyExampleProps {
	title: string;
}

export function MyExample({ title }: MyExampleProps) {
	// @cuilo if styled
	return <h2 className={styles.myExample}>{title}</h2>;
	// @cuilo else
	return <h2 className="my-example">{title}</h2>;
	// @cuilo end
}
