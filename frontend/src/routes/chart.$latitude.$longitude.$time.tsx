import { createFileRoute, Link, useRouter } from "@tanstack/react-router";
import { loadChart } from "../lib/api";
import { AsterDisplay } from '#/components/chart/aster-display.tsx'
import type { Aster, DisplayableAster } from '#/lib/chart.ts'
import { Main } from '#/components/chart/layout.tsx'

export const Route = createFileRoute("/chart/$latitude/$longitude/$time")({
	loader: ({ params }) => loadChart({ data: params }),
	pendingComponent: () => (
		<Main>
			<output>Loading chart…</output>
		</Main>
	),
	errorComponent: ChartError,
	component: Chart,
});

function ChartError() {
	const router = useRouter();
	return (
		<Main>
			<h1>Could not load chart</h1>
			<p role="alert">
				Check the chart URL and make sure the backend is available.
			</p>
			<button type="button" onClick={() => router.invalidate()}>
				Try again
			</button>{" "}
			<Link to="/">Back to form</Link>
		</Main>
	);
}

const ASTERS: DisplayableAster[] = ['Sun', 'Moon', 'Rising', 'Mercury', 'Venus', 'Mars', 'Jupiter', 'Saturn', 'Uranus', 'Neptune', 'Pluto', 'Lilith'];

function Chart() {
	const response = Route.useLoaderData();
	const asters = response.chart.asters;
	return (
		<Main>
			<Link to="/">Back to form</Link>
			<h1>Birth chart</h1>
			{ASTERS.map(aster => <AsterDisplay key={aster} aster={aster} info={aster === 'Rising' ? response.chart.rising : asters[aster]} />)}

			<pre className="chart-json">{JSON.stringify(response, null, 2)}</pre>
		</Main>
	);
}
