import { HttpResponse, http } from "msw";
import { API_BASE } from "#/api/client";
import { DEMO_STUDY_ID, demoCounts, demoStudy } from "./demo";
import {
	datasetAttachment,
	datasetList,
	demographicsReport,
	publishedReport,
	syntheticReport,
} from "./fixtures";

const study = `${API_BASE}/studies/${DEMO_STUDY_ID}`;

/** Read-only demo API: one study with a published dataset report. */
export const handlers = [
	http.get(`${API_BASE}/studies`, () => HttpResponse.json([demoStudy])),
	http.get(study, () => HttpResponse.json(demoStudy)),
	http.get(`${study}/randomizations/counts`, () =>
		HttpResponse.json(demoCounts),
	),
	http.get(`${study}/randomizations`, () => HttpResponse.json([])),
	http.get(`${study}/members`, () => HttpResponse.json([])),
	http.get(`${study}/audit`, () => HttpResponse.json([])),
	http.get(`${study}/datasets`, () =>
		HttpResponse.json(
			datasetList({
				attachments: [
					datasetAttachment({
						id: "attachment-demographics",
						name: "demographics",
						title: "Demographics",
						rowCount: demoCounts.total,
					}),
					datasetAttachment({ rowCount: demoCounts.total }),
				],
			}),
		),
	),
	http.get(`${study}/dataset-reports`, () =>
		HttpResponse.json([
			publishedReport({
				name: "demographics",
				dataset: "demographics",
				report: demographicsReport(),
			}),
			publishedReport({ report: syntheticReport() }),
		]),
	),
	http.all(`${API_BASE}/*`, ({ request }) =>
		request.method === "GET"
			? new HttpResponse(null, { status: 404 })
			: HttpResponse.json({ message: "Demo is read-only" }, { status: 403 }),
	),
];
