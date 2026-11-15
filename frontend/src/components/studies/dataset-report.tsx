import { Database, TriangleAlert } from "lucide-react";
import { useId } from "react";
import type {
	CategoricalField,
	ContinuousField,
	DatasetReport,
	FieldBalance,
} from "#/api/datasets";

function percent(count: number, total: number) {
	return ((count / (total || 1)) * 100).toFixed(1);
}

function SmdCell({
	balance,
	className,
}: {
	balance: FieldBalance;
	className: string;
}) {
	const { smd, imbalanced } = balance;
	return (
		<td
			className={`whitespace-nowrap px-4 text-right tabular-nums ${className}`}
		>
			{smd == null ? (
				"—"
			) : (
				<>
					{smd.toFixed(2)}
					{imbalanced === true && (
						<TriangleAlert
							role="img"
							aria-label="Imbalanced"
							className="ml-1 inline size-4 align-text-bottom text-amber-700 dark:text-amber-400"
						/>
					)}
				</>
			)}
		</td>
	);
}

function CountCell({ count, total }: { count: number | null; total: number }) {
	return (
		<td className="whitespace-nowrap px-4 py-2 text-right tabular-nums">
			{count === null ? (
				"<5"
			) : (
				<>
					{count}{" "}
					<span className="text-muted-foreground">
						({percent(count, total)})
					</span>
				</>
			)}
		</td>
	);
}

function CategoricalRows({
	field,
	report,
}: {
	field: CategoricalField;
	report: DatasetReport;
}) {
	return (
		<>
			<tr className="bg-muted/30">
				<th
					scope="rowgroup"
					colSpan={report.arms.length + 1}
					className="px-4 py-2 text-left font-medium"
				>
					{field.name}, n (%)
				</th>
				{report.smd && <SmdCell balance={field} className="py-2" />}
			</tr>
			{field.categories.map((category) => (
				<tr
					key={
						category.id === null ? "missing-category" : `value:${category.id}`
					}
					className="border-t border-border/50"
				>
					<th
						scope="row"
						className="min-w-64 px-4 py-2 pl-8 text-left font-normal"
					>
						{category.label}
					</th>
					{report.arms.map((arm, index) => (
						<CountCell
							key={arm.id}
							count={category.counts[index]}
							total={arm.count}
						/>
					))}
					{report.smd && <td />}
				</tr>
			))}
		</>
	);
}

function ContinuousRow({
	field,
	report,
}: {
	field: ContinuousField;
	report: DatasetReport;
}) {
	return (
		<tr>
			<th scope="row" className="px-4 py-3 text-left font-medium">
				{field.name}, mean (SD)
				{field.unit ? `, ${field.unit}` : ""}
				{field.note && (
					<span className="block text-xs font-normal text-muted-foreground">
						{field.note}
					</span>
				)}
			</th>
			{report.arms.map((arm, index) => {
				const denominator = field.denominators[index];
				return (
					<td
						key={arm.id}
						className="whitespace-nowrap px-4 py-3 text-right tabular-nums"
					>
						{denominator === null ? (
							"<5"
						) : (
							<>
								{field.means[index]?.toFixed(field.decimals) ?? "—"}{" "}
								<span className="text-muted-foreground">
									(
									{field.standardDeviations[index]?.toFixed(field.decimals) ??
										"—"}
									)
								</span>
							</>
						)}
						{denominator !== null && denominator !== arm.count && (
							<span className="block text-xs text-muted-foreground">
								n = {denominator}
							</span>
						)}
					</td>
				);
			})}
			{report.smd && <SmdCell balance={field} className="py-3" />}
		</tr>
	);
}

function hasSuppressedCell({ fields }: DatasetReport) {
	return fields.some((field) =>
		field.analysis === "categorical"
			? field.categories.some((category) => category.counts.includes(null))
			: field.denominators.includes(null),
	);
}

function Legend({ report }: { report: DatasetReport }) {
	const suppressed = hasSuppressedCell(report);
	if (!report.smd && !suppressed) return null;
	return (
		<p className="mt-2 text-xs text-muted-foreground">
			{report.smd &&
				"SMD: largest absolute standardized mean difference between arms; values above 0.1 are flagged."}
			{report.smd && suppressed && " "}
			{suppressed && "<5: a count from 1 to 4, suppressed to protect privacy."}
		</p>
	);
}

export function DatasetReportCard({ report }: { report: DatasetReport }) {
	const titleId = useId();
	return (
		<section
			className="rounded-lg border bg-card p-5"
			aria-labelledby={titleId}
		>
			<h2
				id={titleId}
				className="flex items-center gap-2 text-sm font-medium text-muted-foreground"
			>
				<Database className="size-4" />
				{report.title}
			</h2>
			<p className="my-4 text-xs text-muted-foreground">{report.description}</p>
			{report.arms.every((arm) => arm.count === 0) ? (
				<p className="py-6 text-center text-sm text-muted-foreground">
					No records in this snapshot.
				</p>
			) : (
				<div className="overflow-x-auto rounded-md border">
					<table className="w-full text-sm">
						<caption className="sr-only">{report.title} by study arm</caption>
						<thead className="border-b bg-muted/50">
							<tr>
								<th
									scope="col"
									className="px-4 py-3 text-left font-medium text-muted-foreground"
								>
									Dataset field
								</th>
								{report.arms.map((arm) => (
									<th
										scope="col"
										key={arm.id}
										className="min-w-44 px-4 py-3 text-right font-medium text-muted-foreground"
									>
										{arm.name}
										<span className="block text-xs font-normal">
											n = {arm.count.toLocaleString()} records
										</span>
									</th>
								))}
								{report.smd && (
									<th
										scope="col"
										className="px-4 py-3 text-right font-medium text-muted-foreground"
									>
										SMD
									</th>
								)}
							</tr>
						</thead>
						{report.fields.map((field) => (
							<tbody key={field.id} className="border-b last:border-b-0">
								{field.analysis === "categorical" ? (
									<CategoricalRows field={field} report={report} />
								) : (
									<ContinuousRow field={field} report={report} />
								)}
							</tbody>
						))}
					</table>
				</div>
			)}
			<Legend report={report} />
		</section>
	);
}
