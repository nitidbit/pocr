import { Toaster as SonnerToaster } from "sonner";

export function Toaster() {
	return (
		<SonnerToaster
			position="bottom-right"
			toastOptions={{
				className:
					"border bg-card text-card-foreground shadow-lg rounded-lg text-sm",
			}}
		/>
	);
}
