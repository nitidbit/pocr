import { createFileRoute } from "@tanstack/react-router";
import { StudyDatasets } from "#/components/studies/study-datasets";

export const Route = createFileRoute("/_app/studies/$studyId/datasets")({
	component: StudyDatasetsPage,
});

function StudyDatasetsPage() {
	const { studyId } = Route.useParams();
	return <StudyDatasets studyId={studyId} />;
}
