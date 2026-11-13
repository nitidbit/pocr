import { Database } from "lucide-react";
import { useId } from "react";
import type { DatasetReport } from "#/api/datasets";

function percent(count: number, total: number) {
	return ((count / (total || 1)) * 100).toFixed(1);
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
							</tr>
						</thead>
						{report.fields.map((field) => (
							<tbody key={field.id} className="border-b last:border-b-0">
								{field.analysis === "categorical" ? (
									<>
										<tr className="bg-muted/30">
											<th
												scope="rowgroup"
												colSpan={report.arms.length + 1}
												className="px-4 py-2 text-left font-medium"
											>
												{field.name}, n (%)
											</th>
										</tr>
										{field.categories.map((category) => (
											<tr
												key={
													category.id === null
														? "missing-category"
														: `value:${category.id}`
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
													<td
														key={arm.id}
														className="whitespace-nowrap px-4 py-2 text-right tabular-nums"
													>
														{category.counts[index]}{" "}
														<span className="text-muted-foreground">
															({percent(category.counts[index], arm.count)})
														</span>
													</td>
												))}
											</tr>
										))}
									</>
								) : (
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
										{report.arms.map((arm, index) => (
											<td
												key={arm.id}
												className="whitespace-nowrap px-4 py-3 text-right tabular-nums"
											>
												{field.means[index]?.toFixed(field.decimals) ?? "—"}{" "}
												<span className="text-muted-foreground">
													(
													{field.standardDeviations[index]?.toFixed(
														field.decimals,
													) ?? "—"}
													)
												</span>
												{field.denominators[index] !== arm.count && (
													<span className="block text-xs text-muted-foreground">
														n = {field.denominators[index]}
													</span>
												)}
											</td>
										))}
									</tr>
								)}
							</tbody>
						))}
					</table>
				</div>
			)}
		</section>
	);
}
