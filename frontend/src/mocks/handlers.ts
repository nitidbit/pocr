import { HttpResponse, http } from "msw";
import { API_BASE } from "#/api/client";
import { DEMO_STUDY_ID, demoBalance, demoCounts, demoStudy } from "./demo";
import {
	datasetAttachment,
	datasetList,
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
	http.get(`${study}/balance`, () => HttpResponse.json(demoBalance)),
	http.get(`${study}/randomizations`, () => HttpResponse.json([])),
	http.get(`${study}/members`, () => HttpResponse.json([])),
	http.get(`${study}/audit`, () => HttpResponse.json([])),
	http.get(`${study}/datasets`, () =>
		HttpResponse.json(
			datasetList({
				attachments: [datasetAttachment({ rowCount: demoCounts.total })],
			}),
		),
	),
	http.get(`${study}/dataset-reports`, () =>
		HttpResponse.json([publishedReport({ report: syntheticReport() })]),
	),
	http.all(`${API_BASE}/*`, ({ request }) =>
		request.method === "GET"
			? new HttpResponse(null, { status: 404 })
			: HttpResponse.json({ message: "Demo is read-only" }, { status: 403 }),
	),
];
